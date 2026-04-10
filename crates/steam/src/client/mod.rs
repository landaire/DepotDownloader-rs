pub mod msg;
pub mod multi;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use bytes::Bytes;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::connection::encryption::SessionCipher;
use crate::error::ConnectionError;
use crate::error::Error;
use crate::error::ParseError;
use crate::generated::CMsgProtoBufHeader;
use crate::messages::EMSG_MASK;
use crate::messages::EMsg;
use crate::messages::PROTO_MASK;
use crate::messages::header::MsgHdr;
use crate::messages::header::MsgHdrProtoBuf;
use crate::transport::Transport;
use crate::types::SteamId;

use self::msg::ClientMsg;

pub struct Disconnected;
pub struct Connected;
pub struct Encrypted;
pub struct LoggedIn;

struct ClientInner {
    transport: Box<dyn Transport>,
    cipher: Mutex<Option<SessionCipher>>,
    steam_id: Mutex<SteamId>,
    session_id: Mutex<i32>,
    next_job_id: AtomicU64,
    pending_jobs: Mutex<HashMap<u64, oneshot::Sender<IncomingMsg>>>,
    event_tx: mpsc::UnboundedSender<IncomingMsg>,
    msg_queue: Mutex<std::collections::VecDeque<IncomingMsg>>,
}

/// A received message from the CM server.
#[derive(Debug)]
pub struct IncomingMsg {
    pub emsg: EMsg,
    pub is_protobuf: bool,
    pub header: CMsgProtoBufHeader,
    pub body: Bytes,
}

/// Response from a service method call. The eresult has already been
/// checked - if you have this value, the call succeeded.
#[derive(Debug)]
pub struct ServiceResponse {
    pub body: Bytes,
}

impl ServiceResponse {
    /// Decode the body as a protobuf message.
    pub fn decode<M: prost::Message + Default>(&self) -> Result<M, prost::DecodeError> {
        M::decode(&self.body[..])
    }

    fn from_incoming(msg: IncomingMsg) -> Result<Self, Error> {
        if let Some(eresult) = msg.header.eresult {
            crate::enums::EResultError::from_i32(eresult)
                .map_err(ConnectionError::ServiceMethodFailed)?;
        }
        Ok(Self { body: msg.body })
    }
}

/// A Steam CM client using the typestate pattern.
///
/// State transitions:
/// `DisconnectedClient` → `SteamClient<Connected>` → `SteamClient<Encrypted>` → `SteamClient<LoggedIn>`
pub struct SteamClient<S> {
    inner: Arc<ClientInner>,
    _state: std::marker::PhantomData<S>,
}

/// The Disconnected state - no connection exists yet.
pub struct DisconnectedClient {
    event_tx: mpsc::UnboundedSender<IncomingMsg>,
}

impl DisconnectedClient {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<IncomingMsg>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        (Self { event_tx }, event_rx)
    }

    /// Connect using any transport implementation.
    pub fn connect(self, transport: impl Transport + 'static) -> SteamClient<Connected> {
        let inner = Arc::new(ClientInner {
            transport: Box::new(transport),
            cipher: Mutex::new(None),
            steam_id: Mutex::new(SteamId::new(0)),
            session_id: Mutex::new(0),
            next_job_id: AtomicU64::new(1),
            pending_jobs: Mutex::new(HashMap::new()),
            event_tx: self.event_tx,
            msg_queue: Mutex::new(std::collections::VecDeque::new()),
        });

        SteamClient {
            inner,
            _state: std::marker::PhantomData,
        }
    }

    /// Connect to a CM server via TCP (convenience for the common case).
    pub async fn connect_tcp(
        self,
        server: &crate::connection::CmServer,
    ) -> Result<SteamClient<Connected>, Error> {
        let tcp = crate::transport::tcp::TcpTransport::connect(server).await?;
        Ok(self.connect(tcp))
    }
}

impl SteamClient<Connected> {
    /// Perform the channel encryption handshake.
    pub async fn encrypt(self) -> Result<SteamClient<Encrypted>, Error> {
        let packet = self.inner.transport.recv().await?;
        let mut reader = &packet[..];

        let hdr = MsgHdr::parse(&mut reader).map_err(ConnectionError::Parse)?;

        if hdr.emsg != EMsg::CHANNEL_ENCRYPT_REQUEST {
            return Err(ConnectionError::UnexpectedEMsg {
                expected: EMsg::CHANNEL_ENCRYPT_REQUEST,
                actual: hdr.emsg,
            }
            .into());
        }

        if reader.len() < 8 {
            return Err(ConnectionError::PacketTooShort {
                need: MsgHdr::SIZE + 8,
                got: packet.len(),
            }
            .into());
        }
        let _protocol_version = read_u32_le(&mut reader)?;
        let _universe = read_u32_le(&mut reader)?;

        let challenge = reader.to_vec();

        let mut session_key = [0u8; 32];
        getrandom::fill(&mut session_key).expect("rng failed");

        let mut blob = Vec::with_capacity(session_key.len() + challenge.len());
        blob.extend_from_slice(&session_key);
        blob.extend_from_slice(&challenge);

        let encrypted_key = crate::crypto::rsa::encrypt_with_steam_public_key(&blob)?;

        let resp_hdr = MsgHdr {
            emsg: EMsg::CHANNEL_ENCRYPT_RESPONSE,
            target_job_id: u64::MAX,
            source_job_id: u64::MAX,
        };

        let packet = build_encrypt_response(&resp_hdr, &encrypted_key);
        self.inner.transport.send(&packet).await?;

        let result_packet = self.inner.transport.recv().await?;
        let mut reader = &result_packet[..];

        let result_hdr = MsgHdr::parse(&mut reader).map_err(ConnectionError::Parse)?;

        if result_hdr.emsg != EMsg::CHANNEL_ENCRYPT_RESULT {
            return Err(ConnectionError::UnexpectedEMsg {
                expected: EMsg::CHANNEL_ENCRYPT_RESULT,
                actual: result_hdr.emsg,
            }
            .into());
        }

        let eresult = read_u32_le(&mut reader)?;
        crate::enums::EResultError::from_i32(eresult as i32)
            .map_err(ConnectionError::EncryptionFailed)?;

        *self.inner.cipher.lock().await = Some(SessionCipher::new(session_key));

        let encrypted_client: SteamClient<Encrypted> = SteamClient {
            inner: self.inner,
            _state: std::marker::PhantomData,
        };

        // Send ClientHello so the server knows we're ready for service method calls
        let hello = crate::generated::CMsgClientHello {
            protocol_version: Some(65581),
            ..Default::default()
        };
        let hello_body = prost::Message::encode_to_vec(&hello);
        let msg = ClientMsg::with_body(EMsg::CLIENT_HELLO, &hello_body);
        encrypted_client.send_msg(&msg).await?;

        Ok(encrypted_client)
    }
}

impl SteamClient<Encrypted> {
    /// Send the ClientLogon message and transition to LoggedIn.
    pub async fn login(
        self,
        msg: ClientMsg<'_>,
    ) -> Result<(SteamClient<LoggedIn>, IncomingMsg), Error> {
        self.send_msg(&msg).await?;

        loop {
            let incoming = self.recv_msg().await?;
            if incoming.emsg == EMsg::CLIENT_LOG_ON_RESPONSE {
                let body: crate::generated::CMsgClientLogonResponse =
                    prost::Message::decode(&incoming.body[..]).unwrap_or_default();
                let eresult = body.eresult.or(incoming.header.eresult).unwrap_or(0);
                tracing::debug!("Logon response eresult: {eresult}");
                if let Err(e) = crate::enums::EResultError::from_i32(eresult) {
                    return Err(ConnectionError::LogonFailed(e).into());
                }

                if let Some(sid) = incoming.header.steamid {
                    *self.inner.steam_id.lock().await = SteamId::new(sid);
                }
                if let Some(session_id) = incoming.header.client_sessionid {
                    *self.inner.session_id.lock().await = session_id;
                }

                return Ok((
                    SteamClient {
                        inner: self.inner,
                        _state: std::marker::PhantomData,
                    },
                    incoming,
                ));
            }
            if self.inner.event_tx.send(incoming).is_err() {
                tracing::trace!("Event receiver dropped, discarding message");
            }
        }
    }

    pub async fn send_msg(&self, msg: &ClientMsg<'_>) -> Result<(), Error> {
        let mut buf = Vec::with_capacity(msg.serialized_len());
        msg.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await
    }

    pub async fn recv_msg(&self) -> Result<IncomingMsg, Error> {
        recv_routed_msg(&self.inner).await
    }

    /// Send a non-authenticated service method call and wait for the response.
    pub async fn call_service_method_non_authed(
        &self,
        method_name: &str,
        body: &[u8],
    ) -> Result<ServiceResponse, Error> {
        let job_id = self.inner.next_job_id.fetch_add(1, Ordering::Relaxed);

        let msg = ClientMsg {
            emsg: EMsg::SERVICE_METHOD_CALL_FROM_CLIENT_NON_AUTHED,
            header: CMsgProtoBufHeader {
                jobid_source: Some(job_id),
                target_job_name: Some(method_name.to_string()),
                ..Default::default()
            },
            body,
        };

        let mut buf = Vec::with_capacity(msg.serialized_len());
        msg.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await?;

        loop {
            let incoming = recv_routed_msg_except(&self.inner, job_id).await?;

            if incoming.emsg == EMsg::SERVICE_METHOD_RESPONSE
                && incoming.header.jobid_target == Some(job_id)
            {
                return ServiceResponse::from_incoming(incoming);
            }

            if self.inner.event_tx.send(incoming).is_err() {
                tracing::trace!("Event receiver dropped, discarding message");
            }
        }
    }

    async fn send_encrypted(&self, payload: &[u8]) -> Result<(), Error> {
        let cipher = self.inner.cipher.lock().await;
        let cipher = cipher
            .as_ref()
            .expect("cipher must be set in Encrypted state");
        let encrypted = cipher.encrypt(payload);
        self.inner.transport.send(&encrypted).await
    }
}

impl SteamClient<LoggedIn> {
    /// Send a protobuf message, filling in SteamID and SessionID from session.
    pub async fn send_msg(&self, msg: &ClientMsg<'_>) -> Result<(), Error> {
        let steam_id = *self.inner.steam_id.lock().await;
        let session_id = *self.inner.session_id.lock().await;

        let mut header = msg.header.clone();
        header.steamid = Some(steam_id.raw());
        header.client_sessionid = Some(session_id);

        let patched = ClientMsg {
            emsg: msg.emsg,
            header,
            body: msg.body,
        };

        let mut buf = Vec::with_capacity(patched.serialized_len());
        patched.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await
    }

    /// Send a service method call and wait for the response.
    ///
    /// Drives the recv loop internally until the matching response arrives.
    /// Other messages received while waiting are forwarded to the event channel.
    pub async fn call_service_method(
        &self,
        method_name: &str,
        body: &[u8],
    ) -> Result<ServiceResponse, Error> {
        tracing::debug!("Calling service method: {method_name}");
        let job_id = self.inner.next_job_id.fetch_add(1, Ordering::Relaxed);
        let steam_id = *self.inner.steam_id.lock().await;
        let session_id = *self.inner.session_id.lock().await;

        let msg = ClientMsg {
            emsg: EMsg::SERVICE_METHOD_CALL_FROM_CLIENT,
            header: CMsgProtoBufHeader {
                steamid: Some(steam_id.raw()),
                client_sessionid: Some(session_id),
                jobid_source: Some(job_id),
                target_job_name: Some(method_name.to_string()),
                ..Default::default()
            },
            body,
        };

        let mut buf = Vec::with_capacity(msg.serialized_len());
        msg.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await?;

        // Drive the recv loop until our job response arrives
        loop {
            let incoming = recv_routed_msg_except(&self.inner, job_id).await?;

            if incoming.emsg == EMsg::SERVICE_METHOD_RESPONSE
                && incoming.header.jobid_target == Some(job_id)
            {
                return ServiceResponse::from_incoming(incoming);
            }

            // Not our response - forward to event channel
            if self.inner.event_tx.send(incoming).is_err() {
                tracing::trace!("Event receiver dropped, discarding message");
            }
        }
    }

    pub async fn recv_msg(&self) -> Result<IncomingMsg, Error> {
        recv_routed_msg(&self.inner).await
    }

    pub async fn send_heartbeat(&self) -> Result<(), Error> {
        let msg = ClientMsg::new(EMsg::CLIENT_HEART_BEAT);
        let mut buf = Vec::with_capacity(msg.serialized_len());
        msg.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await
    }

    async fn send_encrypted(&self, payload: &[u8]) -> Result<(), Error> {
        let cipher = self.inner.cipher.lock().await;
        let cipher = cipher.as_ref().expect("cipher must be set");
        let encrypted = cipher.encrypt(payload);
        self.inner.transport.send(&encrypted).await
    }
}

async fn recv_routed_msg(inner: &ClientInner) -> Result<IncomingMsg, Error> {
    loop {
        {
            let mut queue = inner.msg_queue.lock().await;
            if let Some(msg) = queue.pop_front() {
                if msg.emsg == EMsg::SERVICE_METHOD_RESPONSE
                    && let Some(job_id) = msg.header.jobid_target
                    && let Some(tx) = inner.pending_jobs.lock().await.remove(&job_id)
                {
                    if tx.send(msg).is_err() {
                        tracing::warn!("Job {job_id} receiver dropped, discarding response");
                    }
                    continue;
                }
                return Ok(msg);
            }
        }

        let raw = inner.transport.recv().await?;
        let cipher = inner.cipher.lock().await;
        let payload = match cipher.as_ref() {
            Some(c) => Bytes::from(c.decrypt(&raw).map_err(Error::Crypto)?),
            None => raw,
        };
        drop(cipher);

        let incoming = parse_incoming(payload)?;

        if incoming.emsg == EMsg::MULTI {
            let sub_messages = multi::unpack_multi(&incoming.body)?;
            let mut queue = inner.msg_queue.lock().await;
            for sub in sub_messages {
                match parse_incoming(Bytes::from(sub)) {
                    Ok(parsed) => queue.push_back(parsed),
                    Err(e) => tracing::warn!("Failed to parse sub-message in Multi: {e}"),
                }
            }
            continue;
        }

        if incoming.emsg == EMsg::SERVICE_METHOD_RESPONSE
            && let Some(job_id) = incoming.header.jobid_target
            && let Some(tx) = inner.pending_jobs.lock().await.remove(&job_id)
        {
            if tx.send(incoming).is_err() {
                tracing::warn!("Job {job_id} receiver dropped, discarding response");
            }
            continue;
        }

        return Ok(incoming);
    }
}

/// Like recv_routed_msg but does NOT route the specified job_id -
/// lets it pass through so the caller can handle it directly.
async fn recv_routed_msg_except(
    inner: &ClientInner,
    except_job_id: u64,
) -> Result<IncomingMsg, Error> {
    loop {
        {
            let mut queue = inner.msg_queue.lock().await;
            if let Some(msg) = queue.pop_front() {
                if msg.emsg == EMsg::SERVICE_METHOD_RESPONSE
                    && let Some(job_id) = msg.header.jobid_target
                {
                    // Don't steal our caller's job
                    if job_id != except_job_id
                        && let Some(tx) = inner.pending_jobs.lock().await.remove(&job_id)
                    {
                        if tx.send(msg).is_err() {
                            tracing::warn!("Job {job_id} receiver dropped");
                        }
                        continue;
                    }
                }
                return Ok(msg);
            }
        }

        let raw = inner.transport.recv().await?;
        let cipher = inner.cipher.lock().await;
        let payload = match cipher.as_ref() {
            Some(c) => Bytes::from(c.decrypt(&raw).map_err(Error::Crypto)?),
            None => raw,
        };
        drop(cipher);

        let incoming = parse_incoming(payload)?;

        if incoming.emsg == EMsg::MULTI {
            let sub_messages = multi::unpack_multi(&incoming.body)?;
            let mut queue = inner.msg_queue.lock().await;
            for sub in sub_messages {
                match parse_incoming(Bytes::from(sub)) {
                    Ok(parsed) => queue.push_back(parsed),
                    Err(e) => tracing::warn!("Failed to parse sub-message in Multi: {e}"),
                }
            }
            continue;
        }

        if incoming.emsg == EMsg::SERVICE_METHOD_RESPONSE
            && let Some(job_id) = incoming.header.jobid_target
            && job_id != except_job_id
            && let Some(tx) = inner.pending_jobs.lock().await.remove(&job_id)
        {
            if tx.send(incoming).is_err() {
                tracing::warn!("Job {job_id} receiver dropped");
            }
            continue;
        }

        return Ok(incoming);
    }
}

fn parse_incoming(data: Bytes) -> Result<IncomingMsg, Error> {
    if data.len() < 4 {
        return Err(ConnectionError::PacketTooShort {
            need: 4,
            got: data.len(),
        }
        .into());
    }

    let raw_emsg = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let is_proto = (raw_emsg & PROTO_MASK) != 0;
    let emsg = EMsg::from(raw_emsg & EMSG_MASK);

    if is_proto {
        let mut reader = &data[..];
        let hdr = MsgHdrProtoBuf::parse(&mut reader).map_err(ConnectionError::Parse)?;
        let proto_header: CMsgProtoBufHeader = prost::Message::decode(hdr.header_data)?;

        Ok(IncomingMsg {
            emsg,
            is_protobuf: true,
            header: proto_header,
            body: Bytes::copy_from_slice(reader),
        })
    } else {
        if data.len() < MsgHdr::SIZE {
            return Err(ConnectionError::PacketTooShort {
                need: MsgHdr::SIZE,
                got: data.len(),
            }
            .into());
        }
        Ok(IncomingMsg {
            emsg,
            is_protobuf: false,
            header: CMsgProtoBufHeader::default(),
            body: data.slice(MsgHdr::SIZE..),
        })
    }
}

fn read_u32_le(reader: &mut &[u8]) -> Result<u32, Error> {
    if reader.len() < 4 {
        return Err(ConnectionError::Parse(ParseError::UnexpectedEof).into());
    }
    let val = u32::from_le_bytes([reader[0], reader[1], reader[2], reader[3]]);
    *reader = &reader[4..];
    Ok(val)
}

fn build_encrypt_response(hdr: &MsgHdr, encrypted_key: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(MsgHdr::SIZE + 8 + encrypted_key.len() + 8);
    hdr.write_to(&mut packet).expect("Vec write never fails");
    packet.extend_from_slice(&1u32.to_le_bytes());
    packet.extend_from_slice(&128u32.to_le_bytes());
    packet.extend_from_slice(encrypted_key);
    let crc = crate::util::checksum::Crc32::compute(encrypted_key);
    packet.extend_from_slice(&crc.0.to_le_bytes());
    packet.extend_from_slice(&0u32.to_le_bytes());
    packet
}
