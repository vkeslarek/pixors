use std::fmt;
use std::sync::{Arc, Mutex};

use crate::codec::PageStream;
use pixors_engine::error::Error;
use pixors_engine::stage::{DataKind, OutPortSpecification, PortDeclaration, PortGroup, ProcessorContext, Producer};

static IMG_STREAM_OUTPUTS: &[PortDeclaration] = &[PortDeclaration { name: "scanline", kind: DataKind::ScanLine }];
static IMG_STREAM_OUT_PORTS: OutPortSpecification = OutPortSpecification { ports: PortGroup::Fixed(IMG_STREAM_OUTPUTS) };

#[derive(Clone)]
pub struct ImageStreamSource {
    pub stream: Arc<Mutex<Option<Box<dyn PageStream>>>>,
    pub image_height: u32,
}

impl fmt::Debug for ImageStreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageStreamSource").field("stream", &self.stream.lock().unwrap().is_some()).finish()
    }
}

impl Producer for ImageStreamSource {
    fn kind(&self) -> &'static str { "image_stream" }
    fn out_ports(&self) -> &'static OutPortSpecification { &IMG_STREAM_OUT_PORTS }
    fn source_items(&self) -> usize { self.image_height as usize }

    fn produce(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let mut stream = self.stream.lock().unwrap().take()
            .ok_or_else(|| Error::internal("ImageStreamSource: stream already consumed"))?;
        loop {
            let items = stream.drain(256)?;
            if items.is_empty() { break; }
            for item in items { ctx.emit.emit(item); }
        }
        Ok(())
    }
}
