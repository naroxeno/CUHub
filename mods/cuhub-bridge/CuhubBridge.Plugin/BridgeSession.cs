using System;
using System.Collections.Generic;
using System.Diagnostics;
using BepInEx.Bootstrap;
using BepInEx.Logging;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace CuhubBridge.Plugin
{
    sealed class BridgeSession
    {
        sealed class ReplayEntry
        {
            internal CommandOutcome Outcome;
            internal DateTime At;
        }

        const int DuplicateRetrySeconds = 10;
        const int BusyRetrySeconds = 30;

        readonly ManualLogSource _log;
        readonly CommandRouter _router;
        readonly Dictionary<string, ReplayEntry> _replays = new Dictionary<string, ReplayEntry>(StringComparer.Ordinal);
        readonly Action<string> _clientLog;

        BridgeClient _client;
        ulong _nextId = 1;
        ulong _helloId;
        ulong _pingId;
        int _pingMisses;
        bool _helloForConnection;
        bool _helloAccepted;
        bool _stopped;
        DateTime _handshakeDeadline;
        DateTime _connectDeadline = DateTime.MaxValue;
        DateTime _nextPingAt = DateTime.MaxValue;
        DateTime _retryAt = DateTime.MinValue;

        internal BridgeSession(ManualLogSource log)
        {
            _log = log;
            _router = new CommandRouter(new MpBackend());
            _clientLog = message => _log.LogWarning("CuhubBridge: " + message);
        }

        internal void Start()
        {
            _log.LogInfo("CuhubBridge " + BridgePlugin.PluginVersion + " loaded; connecting to " + Transport.Host + ":" + Transport.Port);
        }

        internal void Pump()
        {
            if (_stopped) return;
            var now = DateTime.UtcNow;
            EnsureClient(now);
            if (_client == null) return;
            DrainIncoming(now);
            if (_stopped) return;
            MaintainConnection(now);
        }

        internal void Stop()
        {
            _stopped = true;
            StopClient();
        }

        void EnsureClient(DateTime now)
        {
            if (_client != null || now < _retryAt) return;
            _client = new BridgeClient(Transport.Host, Transport.Port, _clientLog);
            _client.Start();
            _connectDeadline = now.AddSeconds(60);
        }

        void DrainIncoming(DateTime now)
        {
            for (var i = 0; i < 64; i++)
            {
                if (!_client.TryReceive(out var message)) return;
                Handle(message, now);
                if (_stopped) return;
            }
        }

        void Handle(BridgeMessage message, DateTime now)
        {
            if (message.Kind == MessageKind.Response)
            {
                HandleResponse(message);
            }
            else if (message.Kind == MessageKind.Request)
            {
                HandleRequest(message, now);
            }
        }

        void HandleResponse(BridgeMessage message)
        {
            if (_helloId != 0 && message.Id == _helloId)
            {
                _helloId = 0;
                if (!message.Ok)
                {
                    var code = message.Error?.Code ?? "unknown";
                    var detail = message.Error?.Message ?? string.Empty;
                    _log.LogError("CuhubBridge: host rejected the handshake (" + code + ") " + detail);
                    StopClient();
                    if (code == "duplicate_pid")
                    {
                        _retryAt = DateTime.UtcNow.AddSeconds(DuplicateRetrySeconds);
                        return;
                    }
                    if (code == "busy")
                    {
                        _retryAt = DateTime.UtcNow.AddSeconds(BusyRetrySeconds);
                        return;
                    }
                    _stopped = true;
                    return;
                }
                _helloAccepted = true;
                _nextPingAt = DateTime.UtcNow.AddSeconds(Protocol.PingIntervalSeconds);
                var hostVersion = (message.Result as JObject)?.Value<string>("host_version") ?? "unknown";
                _log.LogInfo("CuhubBridge: connected to cuhub " + hostVersion);
                return;
            }
            if (_pingId != 0 && message.Id == _pingId)
            {
                _pingId = 0;
                _pingMisses = 0;
            }
        }

        void HandleRequest(BridgeMessage message, DateTime now)
        {
            if (_client == null) return;
            if (string.Equals(message.Method, Protocol.Methods.Ping, StringComparison.Ordinal))
            {
                _client.Send(BridgeMessage.Response(message.Id, new JObject()));
                return;
            }
            if (!string.Equals(message.Method, Protocol.Methods.CommandInvoke, StringComparison.Ordinal)) return;
            var parameters = message.Params as JObject ?? new JObject();
            var nonce = parameters.Value<string>("nonce");
            var commandId = parameters.Value<string>("id");
            if (!IsField(nonce, 128) || !IsField(commandId, 128))
            {
                _client.Send(BridgeMessage.Failure(message.Id, Protocol.Errors.InvalidParams, "cmd.invoke requires nonce and id strings"));
                return;
            }
            var nonceKey = "nonce:" + nonce;
            var idKey = "id:" + message.Id;
            if (TryReplay(nonceKey, message.Id, now) || TryReplay(idKey, message.Id, now)) return;
            var commandParameters = parameters["params"] as JObject ?? new JObject();
            var outcome = _router.Execute(commandId, commandParameters);
            var entry = new ReplayEntry { Outcome = outcome, At = now };
            PruneReplays(now);
            _replays[nonceKey] = entry;
            _replays[idKey] = entry;
            _client.Send(BuildResponse(message.Id, outcome));
        }

        bool TryReplay(string key, ulong requestId, DateTime now)
        {
            if (!_replays.TryGetValue(key, out var entry)) return false;
            if ((now - entry.At).TotalSeconds > Protocol.IdReplaySeconds)
            {
                _replays.Remove(key);
                return false;
            }
            _client.Send(BuildResponse(requestId, entry.Outcome));
            return true;
        }

        void PruneReplays(DateTime now)
        {
            if (_replays.Count <= 256) return;
            var stale = new List<string>();
            foreach (var pair in _replays)
            {
                if ((now - pair.Value.At).TotalSeconds > Protocol.IdReplaySeconds) stale.Add(pair.Key);
            }
            foreach (var key in stale) _replays.Remove(key);
            if (_replays.Count > 512) _replays.Clear();
        }

        static BridgeMessage BuildResponse(ulong id, CommandOutcome outcome) =>
            outcome.Ok
                ? BridgeMessage.Response(id, outcome.Result)
                : BridgeMessage.Failure(id, outcome.Error.Code, outcome.Error.Message);

        void MaintainConnection(DateTime now)
        {
            if (_client == null) return;
            if (!_client.IsConnected)
            {
                if (_helloForConnection) Restart("connection lost");
                else if (now >= _connectDeadline) Restart("could not connect to the host");
                return;
            }
            if (!_helloForConnection)
            {
                _helloId = _nextId++;
                _helloForConnection = true;
                _helloAccepted = false;
                _handshakeDeadline = now.AddSeconds(Protocol.HandshakeTimeoutSeconds);
                _client.Send(BridgeMessage.Request(_helloId, Protocol.Methods.Hello, BuildHello()));
                return;
            }
            if (!_helloAccepted)
            {
                if (now >= _handshakeDeadline) Restart("handshake timeout");
                return;
            }
            if (now < _nextPingAt) return;
            if (_pingId != 0)
            {
                _pingMisses++;
                _pingId = 0;
                if (_pingMisses >= Protocol.PingMissLimit)
                {
                    Restart("host missed " + _pingMisses + " pings");
                    return;
                }
            }
            _pingId = _nextId++;
            _client.Send(BridgeMessage.Request(_pingId, Protocol.Methods.Ping, new JObject()));
            _nextPingAt = now.AddSeconds(Protocol.PingIntervalSeconds);
        }

        void Restart(string reason)
        {
            _log.LogWarning("CuhubBridge: " + reason + "; reconnecting");
            StopClient();
            _retryAt = DateTime.UtcNow.AddSeconds(5);
        }

        void StopClient()
        {
            if (_client != null)
            {
                _client.Stop();
                _client = null;
            }
            _helloId = 0;
            _pingId = 0;
            _pingMisses = 0;
            _helloForConnection = false;
            _helloAccepted = false;
            _connectDeadline = DateTime.MaxValue;
            _handshakeDeadline = DateTime.MaxValue;
            _nextPingAt = DateTime.MaxValue;
        }

        JObject BuildHello()
        {
            var mods = new JArray();
            foreach (var plugin in Chainloader.PluginInfos.Values)
            {
                mods.Add(new JObject
                {
                    ["guid"] = plugin.Metadata.GUID,
                    ["version"] = plugin.Metadata.Version?.ToString() ?? "unknown",
                });
            }
            return new JObject
            {
                ["token"] = Transport.Token,
                ["proto"] = new JObject
                {
                    ["major"] = Protocol.Major,
                    ["minor"] = Protocol.Minor,
                },
                ["caps"] = new JArray(),
                ["game"] = new JObject
                {
                    ["pid"] = CurrentPid(),
                    ["unity_version"] = Application.unityVersion,
                    ["bepinex_version"] = BepInExVersion(),
                },
                ["mods"] = mods,
            };
        }

        static int CurrentPid()
        {
            try
            {
                using (var process = Process.GetCurrentProcess()) return process.Id;
            }
            catch (Exception)
            {
                return 0;
            }
        }

        static string BepInExVersion()
        {
            try
            {
                return typeof(Chainloader).Assembly.GetName().Version?.ToString() ?? "unknown";
            }
            catch (Exception)
            {
                return "unknown";
            }
        }

        static bool IsField(string value, int maxLength)
        {
            if (string.IsNullOrEmpty(value) || value.Length > maxLength) return false;
            foreach (var c in value)
            {
                if (char.IsControl(c)) return false;
            }
            return true;
        }
    }
}
