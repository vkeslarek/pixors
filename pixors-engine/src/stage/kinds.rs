#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Tile,
    TileBlock,
    Neighborhood,
    ScanLine,
}

#[derive(Debug, Clone, Copy)]
pub struct PortDeclaration {
    pub name: &'static str,
    pub kind: DataKind,
}

#[derive(Debug, Clone, Copy)]
pub enum PortGroup {
    Fixed(&'static [PortDeclaration]),
    Variable(&'static PortDeclaration),
}

impl PortGroup {
    pub fn is_empty(&self) -> bool {
        match self {
            PortGroup::Fixed(ports) => ports.is_empty(),
            PortGroup::Variable(_) => false,
        }
    }

    pub fn kind_at(&self, index: usize) -> Option<DataKind> {
        match self {
            PortGroup::Fixed(ports) => ports.get(index).map(|p| p.kind),
            PortGroup::Variable(_) => None,
        }
    }

    pub fn name_at(&self, index: usize) -> Option<&'static str> {
        match self {
            PortGroup::Fixed(ports) => ports.get(index).map(|p| p.name),
            PortGroup::Variable(decl) => Some(decl.name),
        }
    }
}

// ── Port specifications ──────────────────────────────────────────────────────

pub struct InPortSpecification {
    pub ports: PortGroup,
}

pub struct OutPortSpecification {
    pub ports: PortGroup,
}

pub struct InOutPortSpecification {
    pub inputs: PortGroup,
    pub outputs: PortGroup,
}

