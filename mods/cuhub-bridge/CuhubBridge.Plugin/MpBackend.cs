using System;
using System.Collections.Generic;
using System.Net;
using System.Net.Sockets;
using System.Reflection;
using Newtonsoft.Json.Linq;

namespace CuhubBridge.Plugin
{
    sealed class MpBackend
    {
        const string ApiTypeName = "CUCoreLib.Networking.MultiplayerApi";
        const string BridgeTypeName = "CUCoreLib.Networking.MultiplayerBridge";
        const string NetTypeName = "Together.Net";
        const string MultiplayerTypeName = "Together.Multiplayer";
        const string TransportProperty = "Transport";
        const string ReceiversProperty = "BridgeReceiversInstalled";
        const string VersionProperty = "KrokMpVersion";
        const string StatusFieldName = "multiplayerStatusMessage";
        const string PortFieldName = "port";
        const ushort DefaultPort = 7790;
        const int MinPlayerNameLength = 3;
        const int MaxStatusLength = 160;

        static readonly string[] AssemblyNames = { "CUCoreLib", "net.cucorelib" };
        static readonly string[] TogetherAssemblyNames = { "CasualtiesMP" };

        readonly Dictionary<string, PropertyInfo> _properties = new Dictionary<string, PropertyInfo>(StringComparer.Ordinal);
        readonly Dictionary<string, MethodInfo> _methods = new Dictionary<string, MethodInfo>(StringComparer.Ordinal);
        readonly Dictionary<string, FieldInfo> _fields = new Dictionary<string, FieldInfo>(StringComparer.Ordinal);
        Type _apiType;
        Type _bridgeType;
        Type _netType;

        internal CommandOutcome State()
        {
            var api = ApiType();
            if (api == null)
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "CUCoreLib is not loaded (plugin GUID net.cucorelib)");
            try
            {
                var running = ReadBool(api, "IsRunning");
                var server = ReadBool(api, "IsServer");
                var state = new JObject
                {
                    ["available"] = ReadBool(api, "IsAvailable"),
                    ["running"] = running,
                    ["client"] = ReadBool(api, "IsClient"),
                    ["server"] = server,
                    ["host"] = ReadBool(api, "IsHost"),
                    ["bridge_receivers_installed"] = ReadBool(api, ReceiversProperty),
                };
                var version = ReadValue(api, VersionProperty);
                state["krokmp_version"] = version?.ToString();
                state["port"] = running && server ? ListeningPort() : JValue.CreateNull();
                return CommandOutcome.Success(state);
            }
            catch (Exception ex)
            {
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerApi reflection failed: " + Unwrap(ex).Message);
            }
        }

        internal CommandOutcome Start(bool host, string address, string name)
        {
            if (!string.IsNullOrEmpty(name) && name.Length < MinPlayerNameLength)
                return CommandOutcome.Failure(
                    Protocol.Errors.InvalidParams,
                    "params.name must be at least " + MinPlayerNameLength + " characters; CasualtiesMP rejects shorter names");
            if (!TryParsePort(address, out var port))
                return CommandOutcome.Failure(Protocol.Errors.InvalidParams, "params.address has an invalid port: '" + address + "'");
            if (port == 0)
                return CommandOutcome.Failure(
                    Protocol.Errors.InvalidParams,
                    "params.address must not use port 0; the game would pick a port the tunnel cannot follow");
            var bridge = BridgeType();
            if (bridge == null)
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "CUCoreLib is not loaded (plugin GUID net.cucorelib)");
            try
            {
                if (host && !IsHosting(port) && IsPortBusy(port))
                    return CommandOutcome.Failure(Protocol.Errors.Busy, "port " + port + " is already in use by another process");
                var startName = host ? "TryStartLocalQuickTestHost" : "TryStartLocalQuickTestClient";
                var start = FindMethod(bridge, startName, typeof(string));
                if (start == null)
                    return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerBridge." + startName + " is not available");
                if (!string.IsNullOrEmpty(name))
                {
                    var configure = FindMethod(bridge, "TryConfigureLocalIdentity", typeof(string), typeof(string));
                    if (configure == null)
                        return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerBridge.TryConfigureLocalIdentity is not available");
                    if (!(bool)Invoke(configure, name, address))
                        return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerBridge.TryConfigureLocalIdentity returned false");
                }
                if (!(bool)Invoke(start, address))
                    return CommandOutcome.Failure(
                        host && IsPortBusy(port) ? Protocol.Errors.Busy : Protocol.Errors.Unavailable,
                        "MultiplayerBridge." + startName + " returned false" + StatusSuffix());
                return ConfirmStart(host, address, port, startName);
            }
            catch (Exception ex)
            {
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerBridge reflection failed: " + Unwrap(ex).Message);
            }
        }

        CommandOutcome ConfirmStart(bool host, string address, ushort port, string startName)
        {
            var after = " after MultiplayerBridge." + startName + " returned true";
            var net = NetType();
            if (net == null)
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "CasualtiesMP (Together) is not loaded" + after);
            if (!TryReadBool(net, "IsRunning", out var running) || !running)
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "Together.Net.IsRunning is false" + after + StatusSuffix());
            var role = host ? "IsServer" : "IsClient";
            if (!TryReadBool(net, role, out var active) || !active)
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "Together.Net." + role + " is false" + after + StatusSuffix());
            if (host)
            {
                var probe = ProbePort();
                if (probe.Status != PortStatus.Ready)
                    return CommandOutcome.Failure(Protocol.Errors.Unavailable, "cannot read the listening port: " + probe.Detail + after);
                if (probe.Port != port)
                    return CommandOutcome.Failure(
                        Protocol.Errors.Unavailable,
                        "the game listens on port " + probe.Port + " instead of " + port + after);
            }
            return CommandOutcome.Success(new JObject
            {
                ["started"] = true,
                ["host"] = host,
                ["address"] = address,
                ["port"] = (int)port,
            });
        }

        JToken ListeningPort()
        {
            var probe = ProbePort();
            return probe.Status == PortStatus.Ready ? new JValue((int)probe.Port) : JValue.CreateNull();
        }

        PortProbe ProbePort()
        {
            try
            {
                var net = NetType();
                if (net == null)
                    return PortProbe.Unavailable("CasualtiesMP (Together) is not loaded");
                if (!TryReadBool(net, "IsRunning", out var running))
                    return PortProbe.Unavailable("Together.Net.IsRunning is not a bool property");
                if (!running) return PortProbe.NotHosting();
                if (!TryReadBool(net, "IsServer", out var server))
                    return PortProbe.Unavailable("Together.Net.IsServer is not a bool property");
                if (!server) return PortProbe.NotHosting();
                var transport = StaticValue(net, TransportProperty);
                if (transport == null) return PortProbe.Unavailable("Together.Net.Transport is null while IsServer is true");
                var transportType = transport.GetType();
                var field = Field(transportType, PortFieldName, isStatic: false);
                if (field == null || field.FieldType != typeof(ushort))
                    return PortProbe.Unavailable(transportType.Name + " has no public ushort " + PortFieldName + " field");
                var port = (ushort)field.GetValue(transport);
                return port == 0
                    ? PortProbe.Unavailable("Together.Net.Transport.port is 0")
                    : PortProbe.Ready(port);
            }
            catch (Exception ex)
            {
                return PortProbe.Unavailable("Together reflection failed: " + Unwrap(ex).Message);
            }
        }

        bool IsHosting(ushort port)
        {
            var probe = ProbePort();
            return probe.Status == PortStatus.Ready && probe.Port == port;
        }

        static bool IsPortBusy(ushort port) =>
            IsBound(new IPEndPoint(IPAddress.Any, port)) ||
            (Socket.OSSupportsIPv6 && IsBound(new IPEndPoint(IPAddress.IPv6Any, port)));

        static bool IsBound(IPEndPoint endpoint)
        {
            try
            {
                using (var socket = new Socket(endpoint.AddressFamily, SocketType.Dgram, ProtocolType.Udp))
                {
                    socket.Bind(endpoint);
                    return false;
                }
            }
            catch (SocketException ex)
            {
                return ex.SocketErrorCode == SocketError.AddressAlreadyInUse;
            }
            catch (Exception)
            {
                return false;
            }
        }

        static bool TryParsePort(string address, out ushort port)
        {
            port = DefaultPort;
            var text = (address ?? string.Empty).Trim();
            if (text.Length == 0) return true;
            var bracketEnd = text.IndexOf(']');
            var lastColon = text.LastIndexOf(':');
            var digits = string.Empty;
            if (text[0] == '[' && bracketEnd != -1)
            {
                if (lastColon > bracketEnd) digits = text.Substring(lastColon + 1);
            }
            else
            {
                var firstDot = text.IndexOf('.');
                var firstColon = text.IndexOf(':');
                var isIpv4 = firstColon == -1 || (firstDot != -1 && firstDot < firstColon);
                if (isIpv4 && lastColon != -1) digits = text.Substring(lastColon + 1);
            }
            digits = digits.Trim();
            return digits.Length == 0 || ushort.TryParse(digits, out port);
        }

        string StatusSuffix()
        {
            var status = StatusText();
            return status == null ? string.Empty : "; game status: " + status;
        }

        string StatusText()
        {
            try
            {
                var type = FindType(MultiplayerTypeName, TogetherAssemblyNames);
                var text = type == null ? null : StaticValue(type, StatusFieldName) as string;
                if (string.IsNullOrEmpty(text)) return null;
                var single = text.Replace('\n', ' ').Replace('\r', ' ').Trim();
                return single.Length > MaxStatusLength ? single.Substring(0, MaxStatusLength) : single;
            }
            catch (Exception)
            {
                return null;
            }
        }

        Type ApiType() => _apiType ?? (_apiType = FindType(ApiTypeName, AssemblyNames));

        Type BridgeType() => _bridgeType ?? (_bridgeType = FindType(BridgeTypeName, AssemblyNames));

        Type NetType() => _netType ?? (_netType = FindType(NetTypeName, TogetherAssemblyNames));

        static Type FindType(string fullName, string[] assemblyNames)
        {
            foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
            {
                var type = assembly.GetType(fullName, false);
                if (type != null) return type;
            }
            foreach (var name in assemblyNames)
            {
                try
                {
                    var type = Assembly.Load(name).GetType(fullName, false);
                    if (type != null) return type;
                }
                catch (Exception)
                {
                }
            }
            return null;
        }

        object StaticValue(Type type, string name)
        {
            var property = Property(type, name);
            if (property != null) return property.GetValue(null, null);
            return Field(type, name, isStatic: true)?.GetValue(null);
        }

        bool TryReadBool(Type type, string name, out bool value)
        {
            value = false;
            var property = Property(type, name);
            if (property == null || property.PropertyType != typeof(bool)) return false;
            value = property.GetValue(null, null) is bool current && current;
            return true;
        }

        bool ReadBool(Type type, string name)
        {
            if (!TryReadBool(type, name, out var value))
                throw new InvalidOperationException(type.Name + "." + name + " is not a bool property");
            return value;
        }

        object ReadValue(Type type, string name) => Property(type, name)?.GetValue(null, null);

        PropertyInfo Property(Type type, string name)
        {
            var key = type.FullName + "." + name;
            if (_properties.TryGetValue(key, out var cached)) return cached;
            var property = type.GetProperty(name, BindingFlags.Public | BindingFlags.Static);
            if (property != null) _properties[key] = property;
            return property;
        }

        FieldInfo Field(Type type, string name, bool isStatic)
        {
            var key = (isStatic ? "static:" : "instance:") + type.FullName + "." + name;
            if (_fields.TryGetValue(key, out var cached)) return cached;
            var field = type.GetField(name, BindingFlags.Public | (isStatic ? BindingFlags.Static : BindingFlags.Instance));
            if (field != null) _fields[key] = field;
            return field;
        }

        MethodInfo FindMethod(Type type, string name, params Type[] parameters)
        {
            var key = type.FullName + "." + name + "/" + parameters.Length;
            if (_methods.TryGetValue(key, out var cached)) return cached;
            var method = type.GetMethod(name, BindingFlags.Public | BindingFlags.Static, null, parameters, null);
            if (method != null) _methods[key] = method;
            return method;
        }

        static object Invoke(MethodInfo method, params object[] arguments)
        {
            try
            {
                return method.Invoke(null, arguments);
            }
            catch (TargetInvocationException ex)
            {
                throw Unwrap(ex);
            }
        }

        static Exception Unwrap(Exception ex) =>
            ex is TargetInvocationException && ex.InnerException != null ? ex.InnerException : ex;

        enum PortStatus
        {
            NotHosting,
            Ready,
            Unavailable,
        }

        readonly struct PortProbe
        {
            PortProbe(PortStatus status, ushort port, string detail)
            {
                Status = status;
                Port = port;
                Detail = detail;
            }

            internal PortStatus Status { get; }
            internal ushort Port { get; }
            internal string Detail { get; }

            internal static PortProbe NotHosting() => new PortProbe(PortStatus.NotHosting, 0, null);
            internal static PortProbe Ready(ushort port) => new PortProbe(PortStatus.Ready, port, null);
            internal static PortProbe Unavailable(string detail) => new PortProbe(PortStatus.Unavailable, 0, detail);
        }
    }
}
