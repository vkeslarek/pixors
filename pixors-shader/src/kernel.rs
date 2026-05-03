pub enum BindElem {
    PixelRgba8U32,
    PixelRgba32F,
    Raw(u32),
}

pub enum BindAccess {
    Read,
    Write,
    ReadWrite,
}

pub struct ResourceDecl {
    pub name: &'static str,
    pub elem: BindElem,
    pub access: BindAccess,
}

pub enum ParamType {
    U32,
    I32,
    F32,
}

pub struct ParamDecl {
    pub name: &'static str,
    pub ty: ParamType,
}

pub enum DispatchShape {
    PerPixel,
    Pixels {
        width_expr: &'static str,
        height_expr: &'static str,
    },
}

pub enum KernelClass {
    PerPixel,
    Stencil { radius: u32 },
    Custom,
}

pub struct KernelSig {
    pub name: &'static str,
    pub entry: &'static str,
    pub inputs: &'static [ResourceDecl],
    pub outputs: &'static [ResourceDecl],
    pub params: &'static [ParamDecl],
    pub workgroup: (u32, u32, u32),
    pub dispatch: DispatchShape,
    pub class: KernelClass,
    pub body: &'static [u8],
}

pub trait GpuKernel: Send + Sync {
    fn sig(&self) -> &KernelSig;
    fn write_params(&self, dst: &mut [u8]);

    fn fusable_body(&self) -> Option<&'static str> {
        None
    }
}
