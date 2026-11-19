// CuhubBridge.Core: the fixed loopback endpoint and the handshake token shared with the Rust host.

namespace CuhubBridge
{
    public static class Transport
    {
        public const string Host = "127.0.0.1";
        public const int Port = 41297;

        // Keeps accidental cross-version connections out; the loopback listener is the boundary.
        public const string Token = "c0ffee5e1d1a7a9b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f70819a2b";
    }
}
