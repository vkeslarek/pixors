use std::fmt;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::common::image::codec::PageStream;
use pixors_engine::error::Error;
use pixors_engine::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, ProcessorContext, Producer, Stage,
};

static IMG_STREAM_INPUTS: &[PortDeclaration] = &[];
static IMG_STREAM_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "scanline",
    kind: DataKind::ScanLine,
}];
static IMG_STREAM_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(IMG_STREAM_INPUTS),
    outputs: PortGroup::Fixed(IMG_STREAM_OUTPUTS),
};

#[derive(Serialize, Deserialize)]
pub struct ImageStreamSource {
    #[serde(skip, default = "empty_stream")]
    pub stream: Arc<Mutex<Option<Box<dyn PageStream>>>>,
    pub image_height: u32,
}

impl fmt::Debug for ImageStreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageStreamSource")
            .field("stream", &self.stream.lock().unwrap().is_some())
            .finish()
    }
}

fn empty_stream() -> Arc<Mutex<Option<Box<dyn PageStream>>>> {
    Arc::new(Mutex::new(None))
}

impl Clone for ImageStreamSource {
    fn clone(&self) -> Self {
        Self {
            stream: Arc::clone(&self.stream),
            image_height: self.image_height,
        }
    }
}

impl Stage for ImageStreamSource {
    fn kind(&self) -> &'static str {
        "image_stream"
    }
    fn ports(&self) -> &'static PortSpecification {
        &IMG_STREAM_PORTS
    }
    fn producer(&self) -> Option<Box<dyn Producer>> {
        self.stream
            .lock()
            .unwrap()
            .take()
            .map(|s| Box::new(ImageStreamProducer { stream: s }) as Box<dyn Producer>)
    }
    fn source_items(&self) -> usize {
        self.image_height as usize
    }
}

pub struct ImageStreamProducer {
    stream: Box<dyn PageStream>,
}

impl Producer for ImageStreamProducer {
    fn produce(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        loop {
            let items = self.stream.drain(256)?;
            if items.is_empty() {
                break;
            }
            for item in items {
                ctx.emit.emit(item);
            }
        }
        Ok(())
    }
    fn source_items(&self) -> usize {
        0
    }
}
