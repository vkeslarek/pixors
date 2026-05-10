use crate::common::pixel::PixelFormat;

#[derive(Debug, Clone)]
pub enum BindingElement {
    PixelRgba8U32,
    PixelRgba32F,
    Image,
    Raw(u32),
}

#[derive(Debug, Clone)]
pub enum BindingAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone)]
pub struct ResourceDeclaration {
    pub name: &'static str,
    pub element: BindingElement,
    pub access: BindingAccess,
}

#[derive(Debug, Clone)]
pub enum ParameterType {
    U32,
    I32,
    F32,
}

#[derive(Debug, Clone)]
pub struct ParameterDeclaration {
    pub name: &'static str,
    pub kind: ParameterType,
}

#[derive(Debug, Clone)]
pub enum DispatchShape {
    PerPixel,
    Pixels {
        width_expression: &'static str,
        height_expression: &'static str,
    },
}

#[derive(Debug, Clone)]
pub enum KernelClass {
    PerPixel,
    Stencil { radius: u32 },
    Custom,
}

#[derive(Debug, Clone)]
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

    fn signature_for(&self, _fmt: PixelFormat) -> Result<KernelSignature, String> {
        Ok(self.signature().clone())
    }

    fn fusable_body(&self) -> Option<&'static str> {
        None
    }
}
