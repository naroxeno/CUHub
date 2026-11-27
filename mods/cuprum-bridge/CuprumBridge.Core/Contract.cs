// CuprumBridge.Core: protocol constants and the third-party-facing contract.
// No BepInEx or Unity dependency; safe to reference from any CU mod.

namespace CuprumBridge
{
    public enum CommandPolicy
    {
        Read,
        Control,
        Mutate,
    }

    public sealed class CommandDescriptor
    {
        public CommandDescriptor(string id, CommandPolicy policy)
        {
            Id = id;
            Policy = policy;
        }

        public string Id { get; }

        public CommandPolicy Policy { get; }
    }

    public static class Protocol
    {
        public const int Major = 1;
        public const int Minor = 0;
        public const int MaxFrame = 1024 * 1024;
        public const int IdReplaySeconds = 60;
        public const int PingIntervalSeconds = 10;
        public const int PingMissLimit = 3;
        public const int HandshakeTimeoutSeconds = 5;

        public static class Methods
        {
            public const string Hello = "hello";
            public const string Ping = "ping";
            public const string GameState = "game.state";
            public const string ModsLoaded = "mods.loaded";
            public const string CommandInvoke = "cmd.invoke";
            public const string PermissionsChanged = "perms.changed";
            public const string ConfigChanged = "config.changed";
            public const string ModsChanged = "mods.changed";
        }

        public static class Errors
        {
            public const string UnknownCommand = "unknown_command";
            public const string Denied = "denied";
            public const string InvalidParams = "invalid_params";
            public const string Busy = "busy";
            public const string Unavailable = "unavailable";
            public const string Timeout = "timeout";
        }

        public static class Commands
        {
            public const string MpState = "mp.state";
            public const string MpHost = "mp.host";
            public const string MpJoin = "mp.join";
        }
    }

    public static class Api
    {
        public const int ContractVersion = 1;

        public const bool Implemented = true;

        public static string Status =>
            "CuprumBridge.Plugin owns the loopback connection; third-party command registration is not exposed yet.";
    }
}
