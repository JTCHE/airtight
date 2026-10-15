//! A flip-model swapchain shown through DirectComposition, and the
//! Direct2D context that draws into it. Composition swapchains carry
//! per-pixel alpha, so Mica shows through what we leave clear.

use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{Interface, Result};

pub struct Target {
    pub dc: ID2D1DeviceContext,
    swapchain: IDXGISwapChain1,
    size: (u32, u32),
    _composition: (IDCompositionDevice, IDCompositionTarget, IDCompositionVisual),
}

fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut rc = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut rc) };
    ((rc.right - rc.left).max(1) as u32, (rc.bottom - rc.top).max(1) as u32)
}

impl Target {
    pub fn new(factory: &ID2D1Factory1, hwnd: HWND) -> Result<Target> {
        let size = client_size(hwnd);
        let mut device = None;
        // WARP: drawing on the CPU, so the GPU driver never loads for a
        // panel this small, and a busy GPU cannot stall it.
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS;
        unsafe {
            D3D11CreateDevice(None, D3D_DRIVER_TYPE_WARP, HMODULE::default(), flags, None, D3D11_SDK_VERSION, Some(&mut device), None, None)?
        };
        let dxgi: IDXGIDevice1 = device.unwrap().cast()?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: size.0,
            Height: size.1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            ..Default::default()
        };
        let dxgi_factory: IDXGIFactory2 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
        let swapchain = unsafe { dxgi_factory.CreateSwapChainForComposition(&dxgi, &desc, None)? };
        let composition: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi)? };
        let target = unsafe { composition.CreateTargetForHwnd(hwnd, true)? };
        let visual = unsafe { composition.CreateVisual()? };
        unsafe {
            visual.SetContent(&swapchain)?;
            target.SetRoot(&visual)?;
            composition.Commit()?;
        }
        let dc = unsafe { factory.CreateDevice(&dxgi)?.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)? };
        let t = Target { dc, swapchain, size, _composition: (composition, target, visual) };
        t.bind()?;
        Ok(t)
    }

    fn bind(&self) -> Result<()> {
        let surface: IDXGISurface = unsafe { self.swapchain.GetBuffer(0)? };
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        let bitmap = unsafe { self.dc.CreateBitmapFromDxgiSurface(&surface, Some(&props))? };
        unsafe { self.dc.SetTarget(&bitmap) };
        Ok(())
    }

    /// Follow the window's client size. Returns false when the device is lost.
    pub fn fit(&mut self, hwnd: HWND) -> bool {
        let size = client_size(hwnd);
        if size == self.size {
            return true;
        }
        self.size = size;
        unsafe {
            self.dc.SetTarget(None);
            let resized = self.swapchain.ResizeBuffers(0, size.0, size.1, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0));
            resized.is_ok() && self.bind().is_ok()
        }
    }

    pub fn present(&self) -> bool {
        unsafe { self.swapchain.Present(1, DXGI_PRESENT(0)).is_ok() }
    }
}
