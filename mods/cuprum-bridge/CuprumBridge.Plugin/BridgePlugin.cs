using BepInEx;
using UnityEngine;

namespace CuprumBridge.Plugin
{
    [BepInPlugin(PluginGuid, PluginName, PluginVersion)]
    public sealed class BridgePlugin : BaseUnityPlugin
    {
        public const string PluginGuid = "dev.cuprum.bridge";
        public const string PluginName = "CuprumBridge";
        public const string PluginVersion = "0.1.1";

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
