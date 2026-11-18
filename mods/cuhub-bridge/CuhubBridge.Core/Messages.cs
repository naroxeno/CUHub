using System;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace CuhubBridge
{
    public enum MessageKind
    {
        Request,
        Response,
        Event,
    }

    public sealed class BridgeError
    {
        public BridgeError(string code, string message)
        {
            Code = code;
            Message = message ?? string.Empty;
        }

        public string Code { get; }

        public string Message { get; }
    }

    public sealed class BridgeMessage
    {
        const int MaxMethodLength = 256;

        BridgeMessage(MessageKind kind, ulong id, string method, JToken parameters, bool ok, JToken result, BridgeError error)
        {
            Kind = kind;
            Id = id;
            Method = method;
            Params = parameters;
            Ok = ok;
            Result = result;
            Error = error;
        }

        public MessageKind Kind { get; }

        public int Version => Protocol.Major;

        public ulong Id { get; }

        public string Method { get; }

        public JToken Params { get; }

        public bool Ok { get; }

        public JToken Result { get; }

        public BridgeError Error { get; }

        public static BridgeMessage Request(ulong id, string method, JToken parameters = null)
        {
            ValidateMethod(method);
            return new BridgeMessage(MessageKind.Request, id, method, parameters ?? new JObject(), true, null, null);
        }

        public static BridgeMessage Response(ulong id, JToken result = null) =>
            new BridgeMessage(MessageKind.Response, id, null, null, true, result, null);

        public static BridgeMessage Failure(ulong id, string code, string message) =>
            new BridgeMessage(MessageKind.Response, id, null, null, false, null, new BridgeError(code, message));

        public static BridgeMessage Event(string method, JToken parameters = null)
        {
            ValidateMethod(method);
            return new BridgeMessage(MessageKind.Event, 0, method, parameters ?? new JObject(), true, null, null);
        }

        public static bool TryParse(string json, out BridgeMessage message, out string error)
        {
            message = null;
            error = null;
            if (string.IsNullOrEmpty(json))
            {
                error = "empty message";
                return false;
            }
            if (json.Length > Protocol.MaxFrame)
            {
                error = "message exceeds " + Protocol.MaxFrame + " bytes";
                return false;
            }
            JObject root;
            try
            {
                root = JObject.Parse(json);
            }
            catch (JsonException ex)
            {
                error = "invalid JSON: " + ex.Message;
                return false;
            }
            var version = root.Value<int?>("v") ?? 0;
            if (version != Protocol.Major)
            {
                error = "unsupported protocol major " + version;
                return false;
            }
            var kind = root.Value<string>("kind");
            switch (kind)
            {
                case "req":
                {
                    if (!TryReadId(root, out var id)) { error = "req without a valid u64 id"; return false; }
                    var method = root.Value<string>("method");
                    if (!IsValidMethod(method)) { error = "req without a valid method"; return false; }
                    message = new BridgeMessage(MessageKind.Request, id, method, root["params"] ?? new JObject(), true, null, null);
                    return true;
                }
                case "res":
                {
                    if (!TryReadId(root, out var id)) { error = "res without a valid u64 id"; return false; }
                    var okToken = root["ok"];
                    if (okToken == null || okToken.Type != JTokenType.Boolean) { error = "res without a boolean ok"; return false; }
                    var ok = okToken.Value<bool>();
                    BridgeError failure = null;
                    if (!ok)
                    {
                        var body = root["error"] as JObject;
                        var code = body?.Value<string>("code");
                        failure = new BridgeError(string.IsNullOrEmpty(code) ? "unknown" : code, body?.Value<string>("message"));
                    }
                    message = new BridgeMessage(MessageKind.Response, id, null, null, ok, root["result"], failure);
                    return true;
                }
                case "evt":
                {
                    var method = root.Value<string>("method");
                    if (!IsValidMethod(method)) { error = "evt without a valid method"; return false; }
                    message = new BridgeMessage(MessageKind.Event, 0, method, root["params"] ?? new JObject(), true, null, null);
                    return true;
                }
                default:
                    error = "unknown message kind '" + kind + "'";
                    return false;
            }
        }

        public string ToJson()
        {
            var root = new JObject { ["v"] = Protocol.Major };
            switch (Kind)
            {
                case MessageKind.Request:
                    root["kind"] = "req";
                    root["id"] = Id;
                    root["method"] = Method;
                    root["params"] = Params ?? new JObject();
                    break;
                case MessageKind.Response:
                    root["kind"] = "res";
                    root["id"] = Id;
                    root["ok"] = Ok;
                    if (Result != null) root["result"] = Result;
                    if (!Ok)
                    {
                        root["error"] = new JObject
                        {
                            ["code"] = Error?.Code ?? "unknown",
                            ["message"] = Error?.Message ?? string.Empty,
                        };
                    }
                    break;
                case MessageKind.Event:
                    root["kind"] = "evt";
                    root["method"] = Method;
                    root["params"] = Params ?? new JObject();
                    break;
            }
            return root.ToString(Formatting.None);
        }

        static void ValidateMethod(string method)
        {
            if (!IsValidMethod(method)) throw new ArgumentException("method is required", nameof(method));
        }

        static bool IsValidMethod(string method)
        {
            if (string.IsNullOrEmpty(method) || method.Length > MaxMethodLength) return false;
            foreach (var c in method)
            {
                if (c < 0x20 || c > 0x7e) return false;
            }
            return true;
        }

        static bool TryReadId(JObject root, out ulong id)
        {
            id = 0;
            var token = root["id"];
            if (token == null || token.Type != JTokenType.Integer) return false;
            try
            {
                id = token.ToObject<ulong>();
                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }
    }
}
