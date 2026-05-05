use std::cell::RefCell;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::graph::item::Item;
use crate::model::image::decoder::PageStream;
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
    pub stream: std::rc::Rc<RefCell<Option<Box<dyn PageStream>>>>,
}

impl fmt::Debug for ImageStreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageStreamSource")
            .field("stream", &self.stream.borrow().is_some())
            .finish()
    }
}

fn empty_stream() -> std::rc::Rc<RefCell<Option<Box<dyn PageStream>>>> {
    std::rc::Rc::new(RefCell::new(None))
}

impl Clone for ImageStreamSource {
    fn clone(&self) -> Self {
        Self {
            stream: std::rc::Rc::clone(&self.stream),
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
            .borrow_mut()
            .take()
            .map(|s| Box::new(ImageStreamProcessor { stream: s }) as Box<dyn Processor>)
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
