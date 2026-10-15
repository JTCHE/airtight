//! The panel for screen readers, through UI Automation: each receiver row
//! and the menu button are buttons, the volume is a slider. UIA calls in on
//! its own threads, so the providers read a copy of the panel that the UI
//! thread keeps current, and post each action back to the window.

#![allow(non_upper_case_globals)] // the UIA ids are constants named in Win32 case

use super::WM_APP_ACCESS;
use super::gfx::Rect;
use super::panel::{Layout, Panel, Part, state_text};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, VARIANT_FALSE, VARIANT_TRUE, WPARAM};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Variant::{VARENUM, VARIANT, VARIANT_0_0, VT_BOOL, VT_BSTR, VT_I4};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PostMessageW};
use windows::core::{BOOL, BSTR, Error, IUnknown, Interface, Result, implement};

const SLIDER: usize = 1000;
const MORE: usize = 1001;
/// The action of a button, in place of a slider value.
const INVOKE: isize = -1;

struct Item {
    part: Part,
    name: String,
    rect: UiaRect,
}

#[derive(Default)]
pub struct Model {
    items: Vec<Item>,
    focus: Option<Part>,
    volume: u8,
}

pub type Shared = Arc<Mutex<Model>>;

/// The window message's code for a part, and back.
pub fn code(part: Part) -> usize {
    match part {
        Part::Row(i) => i,
        Part::Slider => SLIDER,
        Part::More => MORE,
    }
}

pub fn part(code: usize) -> Part {
    match code {
        SLIDER => Part::Slider,
        MORE => Part::More,
        i => Part::Row(i),
    }
}

/// An action that UIA posted: None to invoke the part, or a slider value.
pub fn action(wp: WPARAM, lp: LPARAM) -> (Part, Option<u8>) {
    (part(wp.0), (lp.0 != INVOKE).then(|| lp.0.clamp(0, 100) as u8))
}

/// Copy what the screen reader needs; the UI thread calls this per paint.
pub fn update(model: &Shared, hwnd: HWND, panel: &Panel, layout: &Layout, scale: f32) {
    let mut window = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut window) };
    let place = |r: &Rect| UiaRect {
        left: f64::from(window.left) + f64::from(r.x * scale),
        top: f64::from(window.top) + f64::from(r.y * scale),
        width: f64::from(r.w * scale),
        height: f64::from(r.h * scale),
    };
    let row = |i: usize| match panel.active(i) {
        true => format!("{}, {}", panel.receivers[i].name, state_text(panel.status.state)),
        false => panel.receivers[i].name.clone(),
    };
    let mut items: Vec<Item> = layout.rows.iter().enumerate().map(|(i, r)| Item { part: Part::Row(i), name: row(i), rect: place(r) }).collect();
    items.push(Item { part: Part::Slider, name: "Volume".into(), rect: place(&layout.slider) });
    items.push(Item { part: Part::More, name: "More options".into(), rect: place(&layout.more) });
    *model.lock().unwrap() = Model { items, focus: panel.focus, volume: panel.volume };
}

/// The answer to WM_GETOBJECT.
pub fn root(hwnd: HWND, model: &Shared) -> IRawElementProviderSimple {
    Root { hwnd: hwnd.0 as isize, model: model.clone() }.into()
}

/// Tell a listening screen reader that the keyboard focus moved.
pub fn focus_moved(hwnd: HWND, model: &Shared, part: Part) {
    if unsafe { UiaClientsAreListening() }.as_bool() {
        let element: IRawElementProviderSimple = Element { hwnd: hwnd.0 as isize, model: model.clone(), part }.into();
        let _ = unsafe { UiaRaiseAutomationEvent(&element, UIA_AutomationFocusChangedEventId) };
    }
}

fn variant(vt: VARENUM, fill: impl FnOnce(&mut VARIANT_0_0)) -> VARIANT {
    let mut v = VARIANT::default();
    let inner = unsafe { &mut v.Anonymous.Anonymous };
    inner.vt = vt;
    fill(inner);
    v
}

fn text(s: &str) -> VARIANT {
    variant(VT_BSTR, |v| v.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(s)))
}

fn number(n: i32) -> VARIANT {
    variant(VT_I4, |v| v.Anonymous.lVal = n)
}

fn flag(b: bool) -> VARIANT {
    variant(VT_BOOL, |v| v.Anonymous.boolVal = if b { VARIANT_TRUE } else { VARIANT_FALSE })
}

/// A null result: "no such element" for UIA.
fn none<T>() -> Result<T> {
    Err(Error::empty())
}

#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IRawElementProviderFragmentRoot)]
struct Root {
    hwnd: isize,
    model: Shared,
}

#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IInvokeProvider, IRangeValueProvider)]
struct Element {
    hwnd: isize,
    model: Shared,
    part: Part,
}

impl Root {
    fn element(&self, part: Option<Part>) -> Result<IRawElementProviderFragment> {
        let Some(part) = part else { return none() };
        Ok(Element { hwnd: self.hwnd, model: self.model.clone(), part }.into())
    }
}

impl IRawElementProviderSimple_Impl for Root_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> Result<IUnknown> {
        none()
    }

    /// The window provides the name and the role.
    fn GetPropertyValue(&self, _: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(VARIANT::default())
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(HWND(self.hwnd as _)) }
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let model = self.model.lock().unwrap();
        let part = match direction {
            NavigateDirection_FirstChild => model.items.first().map(|i| i.part),
            NavigateDirection_LastChild => model.items.last().map(|i| i.part),
            _ => None,
        };
        drop(model);
        self.element(part)
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(UiaRect::default())
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(Root { hwnd: self.hwnd, model: self.model.clone() }.into())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let inside = |r: &UiaRect| x >= r.left && x < r.left + r.width && y >= r.top && y < r.top + r.height;
        let part = self.model.lock().unwrap().items.iter().find(|i| inside(&i.rect)).map(|i| i.part);
        self.element(part)
    }

    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let focus = self.model.lock().unwrap().focus;
        self.element(focus)
    }
}

impl Element {
    fn post(&self, value: isize) {
        let _ = unsafe { PostMessageW(Some(HWND(self.hwnd as _)), WM_APP_ACCESS, WPARAM(code(self.part)), LPARAM(value)) };
    }

    fn sibling(&self, step: isize) -> Option<Part> {
        let model = self.model.lock().unwrap();
        let at = model.items.iter().position(|i| i.part == self.part)? as isize;
        model.items.get(usize::try_from(at + step).ok()?).map(|i| i.part)
    }

    fn root(&self) -> Root {
        Root { hwnd: self.hwnd, model: self.model.clone() }
    }
}

impl IRawElementProviderSimple_Impl for Element_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        let slider = self.part == Part::Slider;
        let me = || Element { hwnd: self.hwnd, model: self.model.clone(), part: self.part };
        match pattern {
            UIA_InvokePatternId if !slider => IInvokeProvider::from(me()).cast(),
            UIA_RangeValuePatternId if slider => IRangeValueProvider::from(me()).cast(),
            _ => none(),
        }
    }

    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let model = self.model.lock().unwrap();
        let name = model.items.iter().find(|i| i.part == self.part).map_or("", |i| i.name.as_str());
        let kind = if self.part == Part::Slider { UIA_SliderControlTypeId } else { UIA_ButtonControlTypeId };
        Ok(match property {
            UIA_NamePropertyId => text(name),
            UIA_ControlTypePropertyId => number(kind.0),
            UIA_HasKeyboardFocusPropertyId => flag(model.focus == Some(self.part)),
            UIA_IsKeyboardFocusablePropertyId | UIA_IsEnabledPropertyId => flag(true),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        none()
    }
}

impl IRawElementProviderFragment_Impl for Element_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let root = self.root();
        match direction {
            NavigateDirection_Parent => Ok(root.into()),
            NavigateDirection_NextSibling => root.element(self.sibling(1)),
            NavigateDirection_PreviousSibling => root.element(self.sibling(-1)),
            _ => none(),
        }
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        unsafe {
            let ids = SafeArrayCreateVector(VT_I4, 0, 2);
            for (i, id) in [UiaAppendRuntimeId as i32, code(self.part) as i32 + 1].iter().enumerate() {
                SafeArrayPutElement(ids, &(i as i32), (id as *const i32).cast())?;
            }
            Ok(ids)
        }
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let model = self.model.lock().unwrap();
        Ok(model.items.iter().find(|i| i.part == self.part).map_or_else(UiaRect::default, |i| UiaRect { ..i.rect }))
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.root().into())
    }
}

impl IInvokeProvider_Impl for Element_Impl {
    fn Invoke(&self) -> Result<()> {
        self.post(INVOKE);
        Ok(())
    }
}

impl IRangeValueProvider_Impl for Element_Impl {
    fn SetValue(&self, value: f64) -> Result<()> {
        self.post(value.round().clamp(0.0, 100.0) as isize);
        Ok(())
    }

    fn Value(&self) -> Result<f64> {
        Ok(f64::from(self.model.lock().unwrap().volume))
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(false.into())
    }

    fn Maximum(&self) -> Result<f64> {
        Ok(100.0)
    }

    fn Minimum(&self) -> Result<f64> {
        Ok(0.0)
    }

    fn LargeChange(&self) -> Result<f64> {
        Ok(10.0)
    }

    fn SmallChange(&self) -> Result<f64> {
        Ok(1.0)
    }
}
