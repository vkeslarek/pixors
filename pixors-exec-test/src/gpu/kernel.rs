pub enum BindingElement {
    PixelRgba8U32,
    PixelRgba32F,
    Raw(u32),
}

pub enum BindingAccess {
    Read,
    Write,
    ReadWrite,
}

pub struct ResourceDeclaration {
    pub name: &'static str,
    pub element: BindingElement,
    pub access: BindingAccess,
}

pub enum ParameterType {
    U32,
    I32,
    F32,
}

pub struct ParameterDeclaration {
    pub name: &'static str,
    pub kind: ParameterType,
}

pub enum DispatchShape {
    PerPixel,
    Pixels {
        width_expression: &'static str,
        height_expression: &'static str,
    },
}

pub enum KernelClass {
    PerPixel,
    Stencil { radius: u32 },
    Custom,
}

pub struct KernelSignature {
    pub name: &'static str,
    pub entry: &'static str,
    pub inputs: &'static [ResourceDeclaration],
    pub outputs: &'static [ResourceDeclaration],
    pub params: &'static [ParameterDeclaration],
    pub workgroup: (u32, u32, u32),
    pub dispatch: DispatchShape,
    pub class: KernelClass,
    pub body: &'static [u8],
}

pub trait GpuKernel: Send + Sync {
    fn signature(&self) -> &KernelSignature;
    fn write_params(&self, destination: &mut [u8]);

    fn fusable_body(&self) -> Option<&'static str> {
        None
    }
}
