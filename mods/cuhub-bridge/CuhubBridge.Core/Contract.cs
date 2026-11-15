// CuhubBridge.Core - protocol constants shared with cuhub (docs/bridge/protocol.md).
// Experimental shell: no transport, no JSON, no runtime. The game-side mods are not adapted yet.

namespace CuhubBridge
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

        public static class Methods
        {
            public const string Hello = "hello";
            public const string Ping = "ping";
            public const string GameState = "game.state";
            public const string ModsLoaded = "mods.loaded";
            public const string CommandInvoke = "cmd.invoke";
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
    }

    public static class Api
    {
        public const int ContractVersion = 1;

        public const bool Implemented = false;

        public static string Status => "Experimental shell: the bridge is not wired up yet.";
    }
}
