//! Bridge host methods on the Hub and the event type the UI can drain.

use std::path::PathBuf;

use serde_json::Value;

use crate::bridge::command::Permissions;
use crate::bridge::server::{
    BridgeConfig, BridgeError, BridgeEvent, BridgeOptions, BridgeServer, BridgeStatus,
};
use crate::hub::{Hub, HubError};

#[derive(Debug, Clone)]
pub enum HubEvent {
    Bridge(BridgeEvent),
}

impl Hub {
    pub fn start_bridge(
        &mut self,
        program_data: impl Into<PathBuf>,
    ) -> Result<BridgeStatus, HubError> {
        if let Some(server) = &self.bridge {
            return Ok(server.status());
        }
        let config = BridgeConfig {
            app_data: self.app_data.clone(),
            program_data: program_data.into(),
            permissions: Permissions::from_settings(&self.settings.bridge),
            options: BridgeOptions::default(),
        };
        let server = BridgeServer::start(config, self.logs.clone())?;
        let status = server.status();
        self.bridge = Some(server);
        Ok(status)
    }

    pub fn stop_bridge(&mut self) -> Result<(), HubError> {
        if let Some(server) = self.bridge.take() {
            server.stop();
        }
        Ok(())
    }

    pub fn bridge_status(&self) -> BridgeStatus {
        self.bridge
            .as_ref()
            .map(BridgeServer::status)
            .unwrap_or_default()
    }

    pub fn invoke_bridge(&self, command: &str, params: Value) -> Result<Value, HubError> {
        let Some(server) = &self.bridge else {
            return Err(HubError::Bridge(BridgeError::Unavailable));
        };
        server.invoke(command, params).map_err(HubError::Bridge)
    }

    pub fn take_events(&mut self) -> Vec<HubEvent> {
        let Some(server) = self.bridge.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        while let Some(event) = server.try_recv() {
            events.push(HubEvent::Bridge(event));
        }
        events
    }
}
