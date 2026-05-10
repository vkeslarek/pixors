use crate::data::device::Device;

use super::actors::{Consumer, Processor, Producer};
use super::kinds::PortGroup;

// ── StageHints ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct StageHints {
    pub device: Device,
    pub preference: Option<Device>,
}

impl Default for StageHints {
    fn default() -> Self {
        Self { device: Device::Cpu, preference: None }
    }
}

impl StageHints {
    pub const fn cpu() -> Self { Self { device: Device::Cpu, preference: None } }
    pub const fn gpu() -> Self { Self { device: Device::Gpu, preference: None } }
    pub const fn either() -> Self { Self { device: Device::Either, preference: None } }
    pub const fn prefer_cpu() -> Self { Self { device: Device::Either, preference: Some(Device::Cpu) } }
    pub const fn prefer_gpu() -> Self { Self { device: Device::Either, preference: Some(Device::Gpu) } }
}

// ── Stage enum ───────────────────────────────────────────────────────────────

pub enum Stage {
    Producer(Box<dyn Producer>),
    Processor(Box<dyn Processor>),
    Consumer(Box<dyn Consumer>),
}

impl Stage {
    pub fn kind(&self) -> &'static str {
        match self {
            Stage::Producer(p) => p.kind(),
            Stage::Processor(p) => p.kind(),
            Stage::Consumer(c) => c.kind(),
        }
    }

    pub fn hints(&self) -> StageHints {
        match self {
            Stage::Producer(p) => p.hints(),
            Stage::Processor(p) => p.hints(),
            Stage::Consumer(c) => c.hints(),
        }
    }

    pub fn output_ports(&self) -> PortGroup {
        match self {
            Stage::Producer(p) => p.out_ports().ports,
            Stage::Processor(p) => p.in_out_ports().outputs,
            Stage::Consumer(_) => PortGroup::Fixed(&[]),
        }
    }

    pub fn input_ports(&self) -> PortGroup {
        match self {
            Stage::Producer(_) => PortGroup::Fixed(&[]),
            Stage::Processor(p) => p.in_out_ports().inputs,
            Stage::Consumer(c) => c.in_ports().ports,
        }
    }

    pub fn work_multiplier(&self) -> f64 {
        match self {
            Stage::Processor(p) => p.work_multiplier(),
            _ => 1.0,
        }
    }

    pub fn source_items(&self) -> usize {
        match self {
            Stage::Producer(p) => p.source_items(),
            _ => 0,
        }
    }
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Stage({})", self.kind())
    }
}
