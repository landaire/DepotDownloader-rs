pub mod msg;
pub mod multi;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::connection::CmServer;
use crate::connection::encryption::SessionCipher;
use crate::connection::framing;
use crate::error::{ConnectionError, Error, ParseError};
use crate::generated::CMsgProtoBufHeader;
use crate::messages::header::{MsgHdr, MsgHdrProtoBuf};
use crate::messages::{EMsg, EMSG_MASK, PROTO_MASK};
use crate::types::SteamId;

use self::msg::ClientMsg;

// ── Typestate markers ────────────────────────────────────────

pub struct Disconnected;
pub struct Connected;
pub struct Encrypted;
pub struct LoggedIn;

// ── Core client ──────────────────────────────────────────────

struct ClientInner {
    stream: Mutex<TcpStream>,
    cipher: Mutex<Option<SessionCipher>>,
    steam_id: Mutex<SteamId>,
    session_id: Mutex<i32>,
    next_job_id: AtomicU64,
    pending_jobs: Mutex<HashMap<u64, oneshot::Sender<IncomingMsg>>>,
    event_tx: mpsc::UnboundedSender<IncomingMsg>,
    /// Queue of messages unpacked from Multi payloads.
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

/// A Steam CM client using the typestate pattern.
///
/// State transitions:
/// `DisconnectedClient` → `SteamClient<Connected>` → `SteamClient<Encrypted>` → `SteamClient<LoggedIn>`
pub struct SteamClient<S> {
    inner: Arc<ClientInner>,
    _state: std::marker::PhantomData<S>,
}

/// The Disconnected state — no connection exists yet.
pub struct DisconnectedClient {
    event_tx: mpsc::UnboundedSender<IncomingMsg>,
}

impl DisconnectedClient {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<IncomingMsg>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        (Self { event_tx }, event_rx)
    }

    pub async fn connect(self, server: &CmServer) -> Result<SteamClient<Connected>, Error> {
        use crate::connection::CmServerAddr;
        use tokio::net::lookup_host;

        let stream = match &server.addr {
            CmServerAddr::Resolved(addr) => TcpStream::connect(addr).await?,
            CmServerAddr::Dns { host, port } => {
                let addr = lookup_host(format!("{host}:{port}"))
                    .await?
                    .next()
                    .ok_or_else(|| {
                        ConnectionError::DnsResolutionFailed { host: host.clone() }
                    })?;
                TcpStream::connect(addr).await?
            }
        };
        stream.set_nodelay(true)?;

        let inner = Arc::new(ClientInner {
            stream: Mutex::new(stream),
            cipher: Mutex::new(None),
            steam_id: Mutex::new(SteamId::new(0)),
            session_id: Mutex::new(0),
            next_job_id: AtomicU64::new(1),
            pending_jobs: Mutex::new(HashMap::new()),
            event_tx: self.event_tx,
            msg_queue: Mutex::new(std::collections::VecDeque::new()),
        });

        Ok(SteamClient {
            inner,
            _state: std::marker::PhantomData,
        })
    }
}

impl SteamClient<Connected> {
    /// Perform the channel encryption handshake.
    pub async fn encrypt(self) -> Result<SteamClient<Encrypted>, Error> {
        let packet = self.recv_raw().await?;
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

        // Remaining bytes are the random challenge from the server
        let challenge = reader.to_vec();

        let mut session_key = [0u8; 32];
        getrandom::fill(&mut session_key).expect("rng failed");

        // RSA-encrypt session_key + challenge concatenated (OAEP-SHA1)
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
        self.send_raw(&packet).await?;

        let result_packet = self.recv_raw().await?;
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

        if eresult != 1 {
            return Err(ConnectionError::EncryptionFailed { eresult }.into());
        }

        *self.inner.cipher.lock().await = Some(SessionCipher::new(session_key));

        Ok(SteamClient {
            inner: self.inner,
            _state: std::marker::PhantomData,
        })
    }

    async fn recv_raw(&self) -> Result<Bytes, Error> {
        read_frame(&self.inner.stream).await
    }

    async fn send_raw(&self, payload: &[u8]) -> Result<(), Error> {
        write_frame_to_stream(&self.inner.stream, payload).await
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
                // eresult lives in the body (CMsgClientLogonResponse), not the header
                let body: crate::generated::CMsgClientLogonResponse =
                    prost::Message::decode(&incoming.body[..])
                        .unwrap_or_default();
                let eresult = body.eresult
                    .or(incoming.header.eresult)
                    .unwrap_or(0);
                if eresult != 1 {
                    return Err(ConnectionError::LogonFailed { eresult }.into());
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
            let _ = self.inner.event_tx.send(incoming);
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
    ///
    /// Used for auth RPCs that happen before logon (e.g., GetPasswordRSAPublicKey).
    pub async fn call_service_method_non_authed(
        &self,
        method_name: &str,
        body: &[u8],
    ) -> Result<IncomingMsg, Error> {
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

        let (tx, rx) = oneshot::channel();
        self.inner.pending_jobs.lock().await.insert(job_id, tx);

        let mut buf = Vec::with_capacity(msg.serialized_len());
        msg.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await?;

        rx.await
            .map_err(|_| ConnectionError::Disconnected.into())
    }

    async fn send_encrypted(&self, payload: &[u8]) -> Result<(), Error> {
        let cipher = self.inner.cipher.lock().await;
        let cipher = cipher.as_ref().expect("cipher must be set in Encrypted state");
        let encrypted = cipher.encrypt(payload);
        write_frame_to_stream(&self.inner.stream, &encrypted).await
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
    pub async fn call_service_method(
        &self,
        method_name: &str,
        body: &[u8],
    ) -> Result<IncomingMsg, Error> {
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

        let (tx, rx) = oneshot::channel();
        self.inner.pending_jobs.lock().await.insert(job_id, tx);

        let mut buf = Vec::with_capacity(msg.serialized_len());
        msg.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await?;

        rx.await
            .map_err(|_| ConnectionError::Disconnected.into())
    }

    /// Receive and decrypt a message.
    ///
    /// Automatically unpacks Multi messages and routes service method responses.
    pub async fn recv_msg(&self) -> Result<IncomingMsg, Error> {
        recv_routed_msg(&self.inner).await
    }

    pub async fn send_heartbeat(&self) -> Result<(), Error> {
        let msg = ClientMsg::new(EMsg::CLIENT_HEART_BEAT);
        self.send_msg_owned(msg).await
    }

    /// Send without needing a mutable reference (doesn't fill session fields).
    async fn send_msg_owned(&self, msg: ClientMsg<'_>) -> Result<(), Error> {
        let mut buf = Vec::with_capacity(msg.serialized_len());
        msg.write_to(&mut buf).expect("Vec write never fails");
        self.send_encrypted(&buf).await
    }

    async fn send_encrypted(&self, payload: &[u8]) -> Result<(), Error> {
        let cipher = self.inner.cipher.lock().await;
        let cipher = cipher.as_ref().expect("cipher must be set");
        let encrypted = cipher.encrypt(payload);
        write_frame_to_stream(&self.inner.stream, &encrypted).await
    }
}

// ── I/O helpers ──────────────────────────────────────────────

/// Read a VT01 frame from the async stream.
async fn read_frame(stream: &Mutex<TcpStream>) -> Result<Bytes, Error> {
    use tokio::io::AsyncReadExt;

    let mut stream = stream.lock().await;

    let mut header = [0u8; 8];
    stream.read_exact(&mut header).await?;

    let len = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let magic = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

    if magic != framing::MAGIC {
        return Err(ConnectionError::BadMagic {
            expected: framing::MAGIC,
            actual: magic,
        }
        .into());
    }

    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload).await?;

    Ok(Bytes::from(payload))
}

/// Write a VT01 frame to the async stream.
async fn write_frame_to_stream(stream: &Mutex<TcpStream>, payload: &[u8]) -> Result<(), Error> {
    use tokio::io::AsyncWriteExt;

    let frame = framing::frame_bytes(payload);
    let mut stream = stream.lock().await;
    stream.write_all(&frame).await?;
    Ok(())
}

/// Parse a decrypted message payload into an IncomingMsg.
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

/// Read the next message from the wire, handling Multi unpacking and job routing.
///
/// Shared by both Encrypted and LoggedIn states.
async fn recv_routed_msg(inner: &ClientInner) -> Result<IncomingMsg, Error> {
    loop {
        // Drain the queue first (from previously unpacked Multi messages)
        {
            let mut queue = inner.msg_queue.lock().await;
            if let Some(msg) = queue.pop_front() {
                // Route service method responses to pending jobs
                if msg.emsg == EMsg::SERVICE_METHOD_RESPONSE {
                    if let Some(job_id) = msg.header.jobid_target {
                        if let Some(tx) = inner.pending_jobs.lock().await.remove(&job_id) {
                            let _ = tx.send(msg);
                            continue;
                        }
                    }
                }
                return Ok(msg);
            }
        }

        // Read a new frame from the wire
        let raw = read_frame(&inner.stream).await?;
        let cipher = inner.cipher.lock().await;
        let payload = match cipher.as_ref() {
            Some(c) => Bytes::from(c.decrypt(&raw).map_err(Error::Crypto)?),
            None => raw,
        };
        drop(cipher);

        let incoming = parse_incoming(payload)?;

        // If it's a Multi message, unpack and queue the inner messages
        if incoming.emsg == EMsg::MULTI {
            let sub_messages = multi::unpack_multi(&incoming.body)?;
            let mut queue = inner.msg_queue.lock().await;
            for sub in sub_messages {
                if let Ok(parsed) = parse_incoming(Bytes::from(sub)) {
                    queue.push_back(parsed);
                }
            }
            continue;
        }

        // Route service method responses
        if incoming.emsg == EMsg::SERVICE_METHOD_RESPONSE {
            if let Some(job_id) = incoming.header.jobid_target {
                if let Some(tx) = inner.pending_jobs.lock().await.remove(&job_id) {
                    let _ = tx.send(incoming);
                    continue;
                }
            }
        }

        return Ok(incoming);
    }
}

/// Read a little-endian u32 from a byte slice, advancing the cursor.
fn read_u32_le(reader: &mut &[u8]) -> Result<u32, Error> {
    if reader.len() < 4 {
        return Err(ConnectionError::Parse(ParseError::UnexpectedEof).into());
    }
    let val = u32::from_le_bytes([reader[0], reader[1], reader[2], reader[3]]);
    *reader = &reader[4..];
    Ok(val)
}

/// Build the ChannelEncryptResponse packet.
fn build_encrypt_response(hdr: &MsgHdr, encrypted_key: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(MsgHdr::SIZE + 8 + encrypted_key.len() + 8);
    hdr.write_to(&mut packet).expect("Vec write never fails");
    packet.extend_from_slice(&1u32.to_le_bytes()); // protocol_version
    packet.extend_from_slice(&128u32.to_le_bytes()); // key_size
    packet.extend_from_slice(encrypted_key);
    let crc = crate::util::checksum::Crc32::compute(encrypted_key);
    packet.extend_from_slice(&crc.0.to_le_bytes());
    packet.extend_from_slice(&0u32.to_le_bytes()); // reserved
    packet
}
