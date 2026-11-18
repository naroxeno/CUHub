using System.Collections.Generic;
using Newtonsoft.Json.Linq;

namespace CuhubBridge.Plugin
{
    sealed class CommandRouter
    {
        static readonly HashSet<string> DeclaredCommands = new HashSet<string>
        {
            Protocol.Commands.MpState,
            Protocol.Commands.MpHost,
            Protocol.Commands.MpJoin,
            "mp.leave",
            "mp.kick",
            "save.list",
            "save.current",
            "save.save",
            "save.load",
            "save.backup",
            "save.restore",
            "skin.current",
            "skin.list",
            "skin.apply",
            "skin.reload",
        };

        readonly MpBackend _mp;

        internal CommandRouter(MpBackend mp)
        {
            _mp = mp;
        }

        internal CommandOutcome Execute(string commandId, JObject parameters)
        {
            switch (commandId)
            {
                case Protocol.Commands.MpState:
                    return _mp.State();
                case Protocol.Commands.MpHost:
                    return Start(host: true, parameters);
                case Protocol.Commands.MpJoin:
                    return Start(host: false, parameters);
                default:
                    return DeclaredCommands.Contains(commandId)
                        ? CommandOutcome.Failure(
                            Protocol.Errors.Unavailable,
                            "command '" + commandId + "' declared but not implemented in this build")
                        : CommandOutcome.Failure(
                            Protocol.Errors.UnknownCommand,
                            "unknown command '" + commandId + "'");
            }
        }

        CommandOutcome Start(bool host, JObject parameters)
        {
            var name = host ? Protocol.Commands.MpHost : Protocol.Commands.MpJoin;
            var address = parameters.Value<string>("address");
            if (!IsText(address, 128) || address.Trim().Length == 0)
                return CommandOutcome.Failure(Protocol.Errors.InvalidParams, name + " requires params.address (non-empty, up to 128 chars)");
            address = address.Trim();
            foreach (var c in address)
            {
                if (char.IsWhiteSpace(c) || char.IsControl(c))
                    return CommandOutcome.Failure(Protocol.Errors.InvalidParams, name + " params.address must not contain whitespace");
            }
            var playerName = parameters.Value<string>("name");
            if (playerName != null)
            {
                if (!IsText(playerName, 64))
                    return CommandOutcome.Failure(Protocol.Errors.InvalidParams, name + " params.name must be at most 64 chars");
                playerName = playerName.Trim();
                if (playerName.Length == 0) playerName = null;
            }
            return _mp.Start(host, address, playerName);
        }

        static bool IsText(string value, int maxLength)
        {
            if (value == null || value.Length == 0 || value.Length > maxLength) return false;
            foreach (var c in value)
            {
                if (char.IsControl(c)) return false;
            }
            return true;
        }
    }

    sealed class CommandOutcome
    {
        CommandOutcome(bool ok, JToken result, BridgeError error)
        {
            Ok = ok;
            Result = result;
            Error = error;
        }

        internal bool Ok { get; }

        internal JToken Result { get; }

        internal BridgeError Error { get; }

        internal static CommandOutcome Success(JToken result) => new CommandOutcome(true, result, null);

        internal static CommandOutcome Failure(string code, string message) =>
            new CommandOutcome(false, null, new BridgeError(code, message));
    }
}
