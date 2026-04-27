// plugin/registry.rs — Plugin registry: register, lookup, dispatch
// Spec §6.1, §6.2

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};
use crate::context::AppContext;
use super::{PhotonPlugin, ArgMap, PluginOutput, PluginError};

// ── Plugin registry ───────────────────────────────────────────────────────────

pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, Arc<dyn PhotonPlugin>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
        }
    }

    // Register a plugin — name is normalized to lowercase for lookup
    pub fn register(&self, plugin: Arc<dyn PhotonPlugin>) {
        let name = plugin.name().to_lowercase();
        info!("Registering plugin: {} v{}", plugin.name(), plugin.version());
        self.plugins
            .write()
            .expect("plugin registry lock poisoned")
            .insert(name, plugin);
    }

    // Look up a plugin by name (case-insensitive)
    pub fn get(&self, name: &str) -> Option<Arc<dyn PhotonPlugin>> {
        self.plugins
            .read()
            .expect("plugin registry lock poisoned")
            .get(&name.to_lowercase())
            .cloned()
    }

    // Dispatch a command by name with args against the provided context
    pub fn dispatch(
        &self,
        ctx:     &mut AppContext,
        command: &str,
        args:    &ArgMap,
    ) -> Result<PluginOutput, PluginError> {
        match self.get(command) {
            Some(plugin) => {
                info!("Dispatching: {}", command);
                plugin.execute(ctx, args)
            }
            None => {
                warn!("Unknown command: {}", command);
                Err(PluginError::new(
                    "UNKNOWN_COMMAND",
                    &format!("Unknown command: '{}'. Type Help for a command list.", command),
                ))
            }
        }
    }

    // List all registered plugin names
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.plugins
            .read()
            .expect("plugin registry lock poisoned")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    // List all registered plugins with name, version, and type
    pub fn list_with_details(&self) -> Vec<serde_json::Value> {
        let guard = self.plugins
            .read()
            .expect("plugin registry lock poisoned");
        let mut plugins: Vec<serde_json::Value> = guard.values().map(|p| {
            serde_json::json!({
                "name":        p.name(),
                "version":     p.version(),
                "plugin_type": p.plugin_type(),
            })
        }).collect();
        plugins.sort_by(|a, b| {
            a["name"].as_str().unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        plugins
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
