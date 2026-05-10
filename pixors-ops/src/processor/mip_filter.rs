use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{
    DataKind, InOutPortSpecification, PortDeclaration, PortGroup, Processor, ProcessorContext,
    StageHints,
};
static IN: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static OUT: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static PORTS: InOutPortSpecification = InOutPortSpecification {
    inputs: PortGroup::Fixed(IN),
    outputs: PortGroup::Fixed(OUT),
};

#[derive(Debug, Clone)]
pub struct MipFilter {
    pub mip_level: u32,
}

impl Processor for MipFilter {
    fn kind(&self) -> &'static str {
        "mip_filter"
    }
    fn in_out_ports(&self) -> &'static InOutPortSpecification {
        &PORTS
    }
    fn hints(&self) -> StageHints {
        StageHints::either()
    }

    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let tile = ProcessorContext::take_tile(item)?;
        if tile.coord.mip_level == self.mip_level {
            ctx.emit.emit(Item::Tile(tile));
        }
        Ok(())
    }
}
