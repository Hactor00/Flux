#include <libtorrent/session.hpp>
#include <libtorrent/session_params.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/add_torrent_params.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/load_torrent.hpp>
#include <libtorrent/create_torrent.hpp>
#include <libtorrent/posix_disk_io.hpp>
#include <libtorrent/bencode.hpp>
#include <libtorrent/extensions/ut_pex.hpp>
#include <libtorrent/alert_types.hpp>
#include <algorithm>
#include <cstdint>
#include <cstring>
#include <map>
#include <set>
#include <memory>
#include <sstream>
#include <string>
#include <vector>
#include <filesystem>

namespace lt = libtorrent;
#ifdef _WIN32
#define FLUX_API extern "C" __declspec(dllexport)
#else
#define FLUX_API extern "C" __attribute__((visibility("default")))
#endif

namespace {
thread_local std::string failure;
struct Session {
    lt::session engine;
    std::map<std::uint64_t, lt::torrent_handle> handles;
    std::uint64_t next = 1;
    std::string diagnostic;
    std::set<std::uint64_t> flushed;
    explicit Session(const lt::session_params& params) : engine(params) {}
    void drain() {
        std::vector<lt::alert*> alerts;
        engine.pop_alerts(&alerts);
        for (const auto* alert : alerts) {
            if (const auto* flush = lt::alert_cast<lt::cache_flushed_alert>(alert)) {
                for (const auto& [id, handle] : handles) if (handle == flush->handle) flushed.insert(id);
            }
            if (lt::alert_cast<lt::peer_error_alert>(alert) || lt::alert_cast<lt::peer_disconnected_alert>(alert) || lt::alert_cast<lt::listen_failed_alert>(alert)) {
                diagnostic = alert->message().substr(0, 1024);
            }
        }
    }
};
template<typename F, typename R> R protect(F&& operation, R fallback) noexcept {
    try { failure.clear(); return operation(); }
    catch (const std::exception& error) { failure = error.what(); }
    catch (...) { failure = "Native torrent operation failed"; }
    return fallback;
}
std::string quote(const std::string& value) {
    std::string out = "\"";
    for (unsigned char ch : value) {
        if (ch == '"' || ch == '\\') { out += '\\'; out += ch; }
        else if (ch < 32) {
            static const char hex[] = "0123456789abcdef";
            out += "\\u00"; out += hex[ch >> 4]; out += hex[ch & 15];
        } else out += ch;
    }
    return out + '"';
}
std::size_t copy(const std::string& value, char* out, std::size_t capacity) {
    if (out && capacity >= value.size()) std::memcpy(out, value.data(), value.size());
    return value.size();
}
std::string describe(const lt::torrent_info& info) {
    std::ostringstream out;
    auto hashes = info.info_hashes();
    out << "{\"v2\":" << (hashes.has_v2() ? "true" : "false") << ",\"info_hash\":\"";
    if (hashes.has_v2()) out << hashes.v2; else out << hashes.v1;
    out << "\",\"files\":[";
    const auto& files = info.layout();
    if (files.num_files() > 10000) throw std::runtime_error("Torrent exceeds ten thousand files");
    bool first = true;
    for (lt::file_index_t index{0}; index < files.end_file(); ++index) {
        if (files.pad_file_at(index)) continue;
        if (!files.symlink(index).empty()) throw std::runtime_error("Torrent symlinks are forbidden");
        if (!first) out << ',';
        first = false;
        out << "{\"index\":" << static_cast<int>(index) << ",\"path\":" << quote(files.file_path(index))
            << ",\"length\":" << files.file_size(index) << '}';
    }
    out << "]}";
    return out.str();
}
}

struct FluxStats {
    std::uint64_t completed, total, uploaded, download_rate, upload_rate, peers;
    std::uint32_t metadata, finished, paused, failed;
};

FLUX_API std::uint32_t flux_torrent_abi() noexcept { return 1; }
FLUX_API std::size_t flux_torrent_error(char* output, std::size_t capacity) noexcept {
    return copy(failure, output, capacity);
}
FLUX_API void* flux_torrent_session(std::uint64_t download_limit, std::uint64_t upload_limit, bool discovery) noexcept {
    return protect([&]() -> void* {
        lt::settings_pack settings;
        settings.set_str(lt::settings_pack::user_agent, "Flux/2.3.0");
        settings.set_bool(lt::settings_pack::enable_dht, discovery);
        settings.set_bool(lt::settings_pack::enable_lsd, discovery);
        settings.set_bool(lt::settings_pack::enable_upnp, false);
        settings.set_bool(lt::settings_pack::enable_natpmp, false);
        settings.set_int(lt::settings_pack::download_rate_limit, static_cast<int>(download_limit));
        settings.set_int(lt::settings_pack::upload_rate_limit, static_cast<int>(upload_limit));
        settings.set_int(lt::settings_pack::connections_limit, 500);
        settings.set_int(lt::settings_pack::alert_queue_size, 128);
        settings.set_int(lt::settings_pack::alert_mask, static_cast<int>(static_cast<std::uint32_t>(lt::alert_category::error | lt::alert_category::peer | lt::alert_category::status | lt::alert_category::storage)));
        settings.set_str(lt::settings_pack::listen_interfaces, discovery ? "0.0.0.0:0,[::]:0" : "127.0.0.1:0");
        return new Session(lt::session_params(settings));
    }, static_cast<void*>(nullptr));
}
FLUX_API void flux_torrent_destroy(void* pointer) noexcept {
    if (!pointer) return;
    try { delete static_cast<Session*>(pointer); } catch (...) {}
}
FLUX_API std::size_t flux_torrent_describe(const char* bytes, std::size_t size, char* output, std::size_t capacity) noexcept {
    return protect([&]() {
        auto parameters = lt::load_torrent_buffer(lt::span<const char>(bytes, size));
        return copy(describe(*parameters.ti), output, capacity);
    }, std::size_t(0));
}
FLUX_API std::uint64_t flux_torrent_add(void* pointer, const char* source, std::size_t size, bool magnet, const char* path) noexcept {
    return protect([&]() {
        auto& session = *static_cast<Session*>(pointer);
        lt::error_code error;
        lt::add_torrent_params parameters;
        if (magnet) parameters = lt::parse_magnet_uri(std::string(source, size), error);
        else parameters = lt::load_torrent_buffer(lt::span<const char>(source, size));
        if (error) throw std::runtime_error(error.message());
        parameters.save_path = path;
        parameters.flags &= ~lt::torrent_flags::auto_managed;
        parameters.flags &= ~lt::torrent_flags::paused;
        parameters.flags |= lt::torrent_flags::upload_mode;
        parameters.flags |= lt::torrent_flags::duplicate_is_error;
        auto handle = session.engine.add_torrent(parameters, error);
        if (error) throw std::runtime_error(error.message());
        const auto id = session.next++;
        session.handles.emplace(id, handle);
        return id;
    }, std::uint64_t(0));
}
FLUX_API std::size_t flux_torrent_metadata(void* pointer, std::uint64_t id, char* output, std::size_t capacity) noexcept {
    return protect([&]() {
        auto info = static_cast<Session*>(pointer)->handles.at(id).torrent_file();
        if (!info) return std::size_t(0);
        return copy(describe(*info), output, capacity);
    }, std::size_t(0));
}
FLUX_API bool flux_torrent_start(void* pointer, std::uint64_t id, const char* path, const std::uint32_t* selected, std::size_t count, std::uint32_t download, std::uint32_t upload, std::uint32_t peers) noexcept {
    return protect([&]() {
        auto handle = static_cast<Session*>(pointer)->handles.at(id);
        auto info = handle.torrent_file();
        if (!info) throw std::runtime_error("Torrent metadata is not ready");
        std::vector<lt::download_priority_t> priorities(info->layout().num_files(), count ? lt::dont_download : lt::default_priority);
        for (std::size_t i = 0; i < count; ++i) {
            if (selected[i] >= priorities.size()) throw std::runtime_error("Torrent file index is out of range");
            priorities[selected[i]] = lt::default_priority;
        }
        handle.prioritize_files(priorities);
        handle.set_download_limit(download);
        handle.set_upload_limit(upload);
        handle.set_max_connections(peers);
        handle.move_storage(path);
        handle.force_recheck();
        handle.unset_flags(lt::torrent_flags::upload_mode);
        handle.resume();
        return true;
    }, false);
}
FLUX_API bool flux_torrent_stats(void* pointer, std::uint64_t id, FluxStats* output) noexcept {
    return protect([&]() {
        const auto status = static_cast<Session*>(pointer)->handles.at(id).status();
        *output = {static_cast<std::uint64_t>(std::max<std::int64_t>(0, status.total_wanted_done)),
            static_cast<std::uint64_t>(std::max<std::int64_t>(0, status.total_wanted)),
            static_cast<std::uint64_t>(std::max<std::int64_t>(0, status.all_time_upload)),
            static_cast<std::uint64_t>(std::max(0, status.download_payload_rate)),
            static_cast<std::uint64_t>(std::max(0, status.upload_payload_rate)),
            static_cast<std::uint64_t>(std::max(0, status.num_peers)),
            status.has_metadata, status.is_finished, bool(status.flags & lt::torrent_flags::paused), bool(status.errc)};
        if (status.errc) failure = status.errc.message();
        static_cast<Session*>(pointer)->drain();
        return true;
    }, false);
}
FLUX_API std::size_t flux_torrent_diagnostic(void* pointer, char* output, std::size_t capacity) noexcept {
    return copy(static_cast<Session*>(pointer)->diagnostic, output, capacity);
}
FLUX_API bool flux_torrent_peer(void* pointer, std::uint64_t id, const char* host, std::uint16_t port) noexcept {
    return protect([&]() {
        lt::error_code error;
        auto address = lt::make_address(host, error);
        if (error) throw std::runtime_error(error.message());
        static_cast<Session*>(pointer)->handles.at(id).connect_peer(lt::tcp::endpoint(address, port));
        return true;
    }, false);
}
FLUX_API bool flux_torrent_pause(void* pointer, std::uint64_t id, bool paused) noexcept {
    return protect([&]() {
        auto handle = static_cast<Session*>(pointer)->handles.at(id);
        if (paused) handle.pause(); else handle.resume();
        return true;
    }, false);
}
FLUX_API bool flux_torrent_remove(void* pointer, std::uint64_t id) noexcept {
    return protect([&]() {
        auto& session = *static_cast<Session*>(pointer);
        session.engine.remove_torrent(session.handles.at(id));
        session.handles.erase(id);
        session.flushed.erase(id);
        return true;
    }, false);
}
FLUX_API bool flux_torrent_flush(void* pointer, std::uint64_t id) noexcept {
    return protect([&]() {
        auto& session = *static_cast<Session*>(pointer);
        session.drain();
        session.flushed.erase(id);
        session.handles.at(id).flush_cache();
        return true;
    }, false);
}
FLUX_API std::int32_t flux_torrent_flushed(void* pointer, std::uint64_t id) noexcept {
    return protect([&]() -> std::int32_t {
        auto& session = *static_cast<Session*>(pointer);
        session.drain();
        const auto status = session.handles.at(id).status();
        if (status.errc) throw std::runtime_error(status.errc.message());
        return session.flushed.count(id) ? 1 : 0;
    }, std::int32_t(-1));
}
FLUX_API std::uint16_t flux_torrent_port(void* pointer) noexcept {
    return protect([&]() { return static_cast<Session*>(pointer)->engine.listen_port(); }, std::uint16_t(0));
}
FLUX_API bool flux_torrent_limits(void* pointer, std::uint32_t download, std::uint32_t upload) noexcept {
    return protect([&]() {
        lt::settings_pack settings;
        settings.set_int(lt::settings_pack::download_rate_limit, static_cast<int>(download));
        settings.set_int(lt::settings_pack::upload_rate_limit, static_cast<int>(upload));
        static_cast<Session*>(pointer)->engine.apply_settings(settings);
        return true;
    }, false);
}
FLUX_API bool flux_torrent_counts(void* pointer, std::uint64_t* downloading, std::uint64_t* active) noexcept {
    return protect([&]() {
        *downloading = 0; *active = 0;
        for (const auto& entry : static_cast<Session*>(pointer)->handles) {
            const auto status = entry.second.status();
            if (status.errc || bool(status.flags & lt::torrent_flags::paused)) continue;
            ++*active;
            if (status.has_metadata && !status.is_finished && !bool(status.flags & lt::torrent_flags::upload_mode)) ++*downloading;
        }
        return true;
    }, false);
}
FLUX_API std::size_t flux_torrent_create(const char* path, std::uint32_t mode, char* output, std::size_t capacity) noexcept {
    return protect([&]() {
        auto files = lt::list_files(path);
        auto flags = mode == 2 ? lt::create_torrent::v2_only : mode == 1 ? lt::create_torrent::v1_only : lt::create_flags_t{};
        lt::create_torrent torrent(std::move(files), 65536, flags);
        lt::error_code error;
        lt::set_piece_hashes(torrent, std::filesystem::path(path).parent_path().string(),
            lt::settings_pack{}, lt::posix_disk_io_constructor, [](lt::piece_index_t) {}, error);
        if (error) throw std::runtime_error(error.message());
        const auto bytes = torrent.generate_buf();
        if (output && capacity >= bytes.size()) std::memcpy(output, bytes.data(), bytes.size());
        return bytes.size();
    }, std::size_t(0));
}
