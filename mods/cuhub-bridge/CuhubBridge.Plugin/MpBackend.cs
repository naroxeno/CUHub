using System;
using System.Collections.Generic;
using System.Reflection;
using Newtonsoft.Json.Linq;

namespace CuhubBridge.Plugin
{
    sealed class MpBackend
    {
        const string ApiTypeName = "CUCoreLib.Networking.MultiplayerApi";
        const string BridgeTypeName = "CUCoreLib.Networking.MultiplayerBridge";
        const string ReceiversProperty = "BridgeReceiversInstalled";
        const string VersionProperty = "KrokMpVersion";
        static readonly string[] AssemblyNames = { "CUCoreLib", "net.cucorelib" };
        static readonly string[] StateProperties = { "IsAvailable", "IsRunning", "IsClient", "IsServer", "IsHost" };

        readonly Dictionary<string, PropertyInfo> _properties = new Dictionary<string, PropertyInfo>(StringComparer.Ordinal);
        readonly Dictionary<string, MethodInfo> _methods = new Dictionary<string, MethodInfo>(StringComparer.Ordinal);
        Type _apiType;
        Type _bridgeType;

        internal CommandOutcome State()
        {
            var api = ApiType();
            if (api == null)
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "CUCoreLib is not loaded (plugin GUID net.cucorelib)");
            try
            {
                var state = new JObject();
                foreach (var property in StateProperties)
                {
                    state[property.Substring(2).ToLowerInvariant()] = ReadBool(api, property);
                }
                state["bridge_receivers_installed"] = ReadBool(api, ReceiversProperty);
                var version = ReadValue(api, VersionProperty);
                state["krokmp_version"] = version?.ToString();
                return CommandOutcome.Success(state);
            }
            catch (Exception ex)
            {
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerApi reflection failed: " + Unwrap(ex).Message);
            }
        }

        internal CommandOutcome Start(bool host, string address, string name)
        {
            var bridge = BridgeType();
            if (bridge == null)
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "CUCoreLib is not loaded (plugin GUID net.cucorelib)");
            try
            {
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
                    return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerBridge." + startName + " returned false");
                return CommandOutcome.Success(new JObject
                {
                    ["started"] = true,
                    ["host"] = host,
                    ["address"] = address,
                });
            }
            catch (Exception ex)
            {
                return CommandOutcome.Failure(Protocol.Errors.Unavailable, "MultiplayerBridge reflection failed: " + Unwrap(ex).Message);
            }
        }

        Type ApiType() => _apiType ?? (_apiType = FindType(ApiTypeName));

        Type BridgeType() => _bridgeType ?? (_bridgeType = FindType(BridgeTypeName));

        static Type FindType(string fullName)
        {
            foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
            {
                var type = assembly.GetType(fullName, false);
                if (type != null) return type;
            }
            foreach (var name in AssemblyNames)
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

        bool ReadBool(Type type, string name)
        {
            var property = Property(type, name);
            if (property == null || property.PropertyType != typeof(bool))
                throw new InvalidOperationException("MultiplayerApi." + name + " is not a bool property");
            return property.GetValue(null, null) is bool value && value;
        }

        object ReadValue(Type type, string name) => Property(type, name)?.GetValue(null, null);

        PropertyInfo Property(Type type, string name)
        {
            var key = type.FullName + "." + name;
            if (_properties.TryGetValue(key, out var cached)) return cached;
            var property = type.GetProperty(name, BindingFlags.Public | BindingFlags.Static);
            _properties[key] = property;
            return property;
        }

        MethodInfo FindMethod(Type type, string name, params Type[] parameters)
        {
            var key = type.FullName + "." + name + "/" + parameters.Length;
            if (_methods.TryGetValue(key, out var cached)) return cached;
            var method = type.GetMethod(name, BindingFlags.Public | BindingFlags.Static, null, parameters, null);
            _methods[key] = method;
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
    }
}
