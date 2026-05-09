use crate::data::device::Device;
use serde::{Deserialize, Serialize};

use super::actors::{Consumer, Processor, Producer};
use super::kinds::PortSpecification;

// ── StageHints ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct StageHints {
    pub device: Device,
    pub preference: Option<Device>,
}

impl Default for StageHints {
    fn default() -> Self {
        Self {
            device: Device::Cpu,
            preference: None,
        }
    }
}

impl StageHints {
    pub const fn cpu() -> Self {
        Self {
            device: Device::Cpu,
            preference: None,
        }
    }

    pub const fn gpu() -> Self {
        Self {
            device: Device::Gpu,
            preference: None,
        }
    }

    pub const fn either() -> Self {
        Self {
            device: Device::Either,
            preference: None,
        }
    }

    pub const fn prefer_cpu() -> Self {
        Self {
            device: Device::Either,
            preference: Some(Device::Cpu),
        }
    }

    pub const fn prefer_gpu() -> Self {
        Self {
            device: Device::Either,
            preference: Some(Device::Gpu),
        }
    }
}

// ── Stage trait ────────────────────────────────────────────────────────────────

pub trait Stage: Send + Sync + std::fmt::Debug {
    fn kind(&self) -> &'static str;
    fn ports(&self) -> &'static PortSpecification;
    fn hints(&self) -> StageHints {
        StageHints::cpu()
    }
    fn producer(&self) -> Option<Box<dyn Producer>> {
        None
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        None
    }
    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        None
    }
    fn work_multiplier(&self) -> f64 {
        1.0
    }
    fn source_items(&self) -> usize {
        0
    }
}

// ── StageRole ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageRole {
    Source,
    Operation,
    Sink,
}
