use super::Component;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gray<T: Component> {
    pub v: T,
}

impl<T: Component> Gray<T> {
    pub const fn new(v: T) -> Self {
        Self { v }
    }
}

unsafe impl<T: Component> bytemuck::Pod for Gray<T> {}
unsafe impl<T: Component> bytemuck::Zeroable for Gray<T> {}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrayAlpha<T: Component> {
    pub v: T,
    pub a: T,
}

impl<T: Component> GrayAlpha<T> {
    pub const fn new(v: T, a: T) -> Self {
        Self { v, a }
    }
}

unsafe impl<T: Component> bytemuck::Pod for GrayAlpha<T> {}
unsafe impl<T: Component> bytemuck::Zeroable for GrayAlpha<T> {}
