use std::sync::{Arc, Mutex};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::graph::item::Item;
use crate::model::io::PageStream;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
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
    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        self.stream
            .lock()
            .unwrap()
            .take()
            .map(|s| Box::new(ImageStreamProcessor { stream: s }) as Box<dyn Processor>)
    }
    fn source_items(&self) -> usize {
        self.image_height as usize
    }
}

pub struct ImageStreamProcessor {
    stream: Box<dyn PageStream>,
}

impl Processor for ImageStreamProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, _item: Item) -> Result<(), Error> {
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
}
