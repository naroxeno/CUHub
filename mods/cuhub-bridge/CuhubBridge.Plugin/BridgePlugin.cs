using BepInEx;
using UnityEngine;

namespace CuhubBridge.Plugin
{
    [BepInPlugin(PluginGuid, PluginName, PluginVersion)]
    public sealed class BridgePlugin : BaseUnityPlugin
    {
        public const string PluginGuid = "dev.cuhub.bridge";
        public const string PluginName = "CuhubBridge";
        public const string PluginVersion = "0.1.0";

        BridgeSession _session;

        void Awake()
        {
            _session = new BridgeSession(Logger);
            _session.Start();
        }

        void Update()
        {
            _session.Pump();
        }

        void OnDestroy()
        {
            _session?.Stop();
            _session = null;
        }
    }
}
