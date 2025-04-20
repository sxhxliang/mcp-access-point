#[warn(dead_code)]
pub mod http_admin;
pub mod validate;
use std::error::Error;

use crate::{
    config,
    plugin::build_plugin,
};


trait PluginValidatable {
    fn validate_plugins(&self) -> Result<(), Box<dyn Error>>;
}

impl PluginValidatable for config::Route {
    fn validate_plugins(&self) -> Result<(), Box<dyn Error>> {
        for (name, value) in &self.plugins {
            build_plugin(name, value.clone())?;
        }
        Ok(())
    }
}

impl PluginValidatable for config::Service {
    fn validate_plugins(&self) -> Result<(), Box<dyn Error>> {
        for (name, value) in &self.plugins {
            build_plugin(name, value.clone())?;
        }
        Ok(())
    }
}

impl PluginValidatable for config::GlobalRule {
    fn validate_plugins(&self) -> Result<(), Box<dyn Error>> {
        for (name, value) in &self.plugins {
            build_plugin(name, value.clone())?;
        }
        Ok(())
    }
}
