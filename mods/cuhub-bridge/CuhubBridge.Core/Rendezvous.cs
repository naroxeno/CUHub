using System;
using System.IO;
using Newtonsoft.Json.Linq;

namespace CuhubBridge
{
    public sealed class RendezvousFile
    {
        public RendezvousFile(string host, int port, string token)
        {
            Host = host;
            Port = port;
            Token = token;
        }

        public string Host { get; }

        public int Port { get; }

        public string Token { get; }
    }

    public static class Rendezvous
    {
        public const string DirectoryName = "CasualtiesHub";
        public const string FileName = "bridge.json";
        public const string Loopback = "127.0.0.1";
        const int MaxFileBytes = 64 * 1024;

        public static string DefaultPath()
        {
            var programData = Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData);
            return string.IsNullOrEmpty(programData) ? null : Path.Combine(programData, DirectoryName, FileName);
        }

        public static bool TryRead(out RendezvousFile file) => TryRead(DefaultPath(), out file);

        public static bool TryRead(string path, out RendezvousFile file)
        {
            file = null;
            if (string.IsNullOrEmpty(path)) return false;
            try
            {
                var info = new FileInfo(path);
                if (!info.Exists || info.Length <= 0 || info.Length > MaxFileBytes) return false;
                var root = JObject.Parse(File.ReadAllText(path));
                if (root.Value<int?>("proto") != Protocol.Major) return false;
                if (!(root["endpoint"] is JObject endpoint)) return false;
                if (!string.Equals(endpoint.Value<string>("host"), Loopback, StringComparison.Ordinal)) return false;
                var port = endpoint.Value<int?>("port");
                if (port == null || port.Value <= 0 || port.Value > 65535) return false;
                var token = root.Value<string>("token");
                if (!IsToken(token)) return false;
                file = new RendezvousFile(Loopback, port.Value, token);
                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }

        static bool IsToken(string token)
        {
            if (token == null || token.Length != 64) return false;
            foreach (var c in token)
            {
                if ((c < '0' || c > '9') && (c < 'a' || c > 'f')) return false;
            }
            return true;
        }
    }
}
