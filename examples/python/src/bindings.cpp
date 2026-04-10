#include <nanobind/nanobind.h>
#include <nanobind/stl/string.h>
#include <nanobind/stl/unique_ptr.h>
#include <nanobind/stl/vector.h>
#include <stdexcept>

#include "AppId.hpp"
#include "DepotId.hpp"
#include "ManifestId.hpp"
#include "CellId.hpp"
#include "Runtime.hpp"
#include "CmServerList.hpp"
#include "SteamClient.hpp"
#include "AccessTokenList.hpp"
#include "AppInfoList.hpp"
#include "AppInfoKv.hpp"
#include "CdnServerList.hpp"
#include "CdnClient.hpp"
#include "DepotManifest.hpp"
#include "DepotKey.hpp"
#include "AuthSession.hpp"
#include "AuthTokens.hpp"
#include "RsaPublicKey.hpp"
#include "TokenStore.hpp"
#include "FfiError.hpp"
#include "SteamError.hpp"
#include "GuardType.hpp"

namespace nb = nanobind;
using namespace nb::literals;

template <typename T>
T unwrap(diplomat::result<T, std::unique_ptr<FfiError>>&& r) {
    if (r.is_ok()) return std::move(r).ok().value();
    throw std::runtime_error(std::move(r).err().value()->message());
}

inline void unwrap_void(diplomat::result<std::monostate, std::unique_ptr<FfiError>>&& r) {
    if (!r.is_ok()) throw std::runtime_error(std::move(r).err().value()->message());
}

// Diplomat classes are non-copyable/non-movable and use a custom
// operator delete that calls the Rust destructor. We wrap each in a
// simple holder struct so nanobind can own the unique_ptr.
struct PyRuntime { std::unique_ptr<Runtime> p; };
struct PyCmServerList { std::unique_ptr<CmServerList> p; };
struct PySteamClient { std::unique_ptr<SteamClient> p; };
struct PyAccessTokenList { std::unique_ptr<AccessTokenList> p; };
struct PyAppInfoList { std::unique_ptr<AppInfoList> p; };
struct PyCdnServerList { std::unique_ptr<CdnServerList> p; };
struct PyCdnClient { std::unique_ptr<CdnClient> p; };
struct PyDepotManifest { std::unique_ptr<DepotManifest> p; };
struct PyDepotKey { std::unique_ptr<DepotKey> p; };
struct PyAuthSession { std::unique_ptr<AuthSession> p; };
struct PyAuthTokens { std::unique_ptr<AuthTokens> p; };
struct PyRsaPublicKey { std::unique_ptr<RsaPublicKey> p; };
struct PyTokenStore { std::unique_ptr<TokenStore> p; };

NB_MODULE(steam_ffi_ext, m) {
    m.doc() = "Python bindings for the steam-ffi Rust library (via diplomat + nanobind)";

    nb::class_<PyRuntime>(m, "Runtime")
        .def("__init__", [](PyRuntime *self) {
            new (self) PyRuntime{unwrap(Runtime::new_())};
        });

    nb::class_<PyCmServerList>(m, "CmServerList")
        .def_static("fetch", [](PyRuntime &rt, uint32_t cell_id) {
            auto cid = CellId::new_(cell_id);
            return PyCmServerList{unwrap(CmServerList::fetch(*rt.p, *cid))};
        }, "rt"_a, "cell_id"_a = 0)
        .def_static("defaults", []() {
            return PyCmServerList{CmServerList::defaults()};
        })
        .def("__len__", [](PyCmServerList &s) { return s.p->len(); });

    nb::class_<PySteamClient>(m, "SteamClient")
        .def_static("connect", [](PyRuntime &rt, PyCmServerList &srv, uint32_t idx) {
            return PySteamClient{unwrap(SteamClient::connect(*rt.p, *srv.p, idx))};
        }, "rt"_a, "servers"_a, "server_index"_a = 0)
        .def("login_anonymous", [](PySteamClient &s, PyRuntime &rt, uint32_t cell_id) {
            auto cid = CellId::new_(cell_id);
            unwrap_void(s.p->login_anonymous(*rt.p, *cid));
        }, "rt"_a, "cell_id"_a = 0)
        .def("login_with_token", [](PySteamClient &s, PyRuntime &rt,
                                     std::string user, std::string token, uint32_t cell_id) {
            auto cid = CellId::new_(cell_id);
            unwrap_void(s.p->login_with_token(*rt.p, user, token, *cid));
        }, "rt"_a, "username"_a, "access_token"_a, "cell_id"_a = 0)
        .def("get_access_tokens", [](PySteamClient &s, PyRuntime &rt, std::vector<uint32_t> ids) {
            diplomat::span<const uint32_t> sp(ids.data(), ids.size());
            return PyAccessTokenList{unwrap(s.p->get_access_tokens(*rt.p, sp))};
        }, "rt"_a, "app_ids"_a)
        .def("get_product_info", [](PySteamClient &s, PyRuntime &rt, PyAccessTokenList &tok) {
            return PyAppInfoList{unwrap(s.p->get_product_info(*rt.p, *tok.p))};
        }, "rt"_a, "tokens"_a)
        .def("get_depot_key", [](PySteamClient &s, PyRuntime &rt, uint32_t depot, uint32_t app) {
            auto did = DepotId::new_(depot);
            auto aid = AppId::new_(app);
            return PyDepotKey{unwrap(s.p->get_depot_key(*rt.p, *did, *aid))};
        }, "rt"_a, "depot_id"_a, "app_id"_a)
        .def("get_cdn_servers", [](PySteamClient &s, PyRuntime &rt, uint32_t cell_id) {
            auto cid = CellId::new_(cell_id);
            return PyCdnServerList{unwrap(s.p->get_cdn_servers(*rt.p, *cid))};
        }, "rt"_a, "cell_id"_a = 0)
        .def("get_manifest_request_code", [](PySteamClient &s, PyRuntime &rt,
                                              uint32_t app, uint32_t depot,
                                              uint64_t manifest, std::string branch) {
            auto aid = AppId::new_(app);
            auto did = DepotId::new_(depot);
            auto mid = ManifestId::new_(manifest);
            auto r = s.p->get_manifest_request_code(*rt.p, *aid, *did, *mid, branch);
            return r.is_ok() ? std::move(r).ok().value() : uint64_t(0);
        }, "rt"_a, "app_id"_a, "depot_id"_a, "manifest_id"_a, "branch"_a);

    nb::class_<PyAccessTokenList>(m, "AccessTokenList")
        .def("__len__", [](PyAccessTokenList &s) { return s.p->len(); });

    nb::class_<PyAppInfoList>(m, "AppInfoList")
        .def("__len__", [](PyAppInfoList &s) { return s.p->len(); })
        .def("app_id_at", [](PyAppInfoList &s, uint32_t i) { return s.p->app_id_at(i); }, "index"_a);

    nb::class_<PyCdnServerList>(m, "CdnServerList")
        .def("__len__", [](PyCdnServerList &s) { return s.p->len(); });

    nb::class_<PyCdnClient>(m, "CdnClient")
        .def("__init__", [](PyCdnClient *self) {
            new (self) PyCdnClient{unwrap(CdnClient::new_())};
        })
        .def("download_manifest", [](PyCdnClient &s, PyRuntime &rt, PyCdnServerList &srv,
                                      uint32_t idx, uint32_t depot, uint64_t manifest, uint64_t code) {
            auto did = DepotId::new_(depot);
            auto mid = ManifestId::new_(manifest);
            return PyDepotManifest{unwrap(s.p->download_manifest(*rt.p, *srv.p, idx, *did, *mid, code))};
        }, "rt"_a, "servers"_a, "server_index"_a, "depot_id"_a, "manifest_id"_a, "request_code"_a);

    nb::class_<PyDepotManifest>(m, "DepotManifest")
        .def_prop_ro("file_count", [](PyDepotManifest &s) { return s.p->file_count(); })
        .def_prop_ro("filenames_encrypted", [](PyDepotManifest &s) { return s.p->filenames_encrypted(); })
        .def_prop_ro("total_uncompressed_size", [](PyDepotManifest &s) { return s.p->total_uncompressed_size(); })
        .def_prop_ro("total_compressed_size", [](PyDepotManifest &s) { return s.p->total_compressed_size(); })
        .def_prop_ro("creation_time", [](PyDepotManifest &s) { return s.p->creation_time(); })
        .def("file_name", [](PyDepotManifest &s, uint32_t i) { return s.p->file_name(i); }, "index"_a)
        .def("file_size", [](PyDepotManifest &s, uint32_t i) { return s.p->file_size(i); }, "index"_a)
        .def("file_chunk_count", [](PyDepotManifest &s, uint32_t i) { return s.p->file_chunk_count(i); }, "index"_a)
        .def("decrypt_filenames", [](PyDepotManifest &s, PyDepotKey &k) {
            unwrap_void(s.p->decrypt_filenames(*k.p));
        }, "key"_a);

    nb::class_<PyDepotKey>(m, "DepotKey");

    nb::class_<PyAuthSession>(m, "AuthSession")
        .def("guard_type_count", [](PyAuthSession &s) { return s.p->guard_type_count(); });

    nb::class_<PyAuthTokens>(m, "AuthTokens")
        .def_prop_ro("access_token", [](PyAuthTokens &s) { return s.p->access_token(); })
        .def_prop_ro("refresh_token", [](PyAuthTokens &s) { return s.p->refresh_token(); })
        .def_prop_ro("account_name", [](PyAuthTokens &s) { return s.p->account_name(); });

    nb::class_<PyRsaPublicKey>(m, "RsaPublicKey")
        .def_prop_ro("modulus", [](PyRsaPublicKey &s) { return s.p->modulus(); })
        .def_prop_ro("exponent", [](PyRsaPublicKey &s) { return s.p->exponent(); })
        .def_prop_ro("timestamp", [](PyRsaPublicKey &s) { return s.p->timestamp(); });

    nb::class_<PyTokenStore>(m, "TokenStore")
        .def_static("load_default", []() { return PyTokenStore{TokenStore::load_default()}; })
        .def("has", [](PyTokenStore &s, std::string user) { return s.p->get(user); }, "username"_a)
        .def("set", [](PyTokenStore &s, std::string user, std::string tok) { s.p->set(user, tok); },
             "username"_a, "token"_a)
        .def("save", [](PyTokenStore &s) { unwrap_void(s.p->save()); });
}
