//! Plaid Link in a native window, the way Quicken connects a bank: a sheet
//! that slides down from Checken's own window and shows Plaid's sign-in in a
//! WebKit view (the engine Safari uses). Banks that sign in through their own
//! site (OAuth) open in a further native window, through the web view's
//! window.open handling. On systems other than the Mac, Link opens in the
//! browser instead.

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSApplication, NSBackingStoreType, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL, NSURLRequest};
    use objc2_web_kit::{WKNavigationAction, WKUIDelegate, WKWebView, WKWebViewConfiguration, WKWindowFeatures};
    use std::cell::RefCell;

    pub struct Popups {
        windows: RefCell<Vec<Retained<NSWindow>>>,
    }

    define_class!(
        // Opens the bank's own sign-in pages (window.open from Plaid Link)
        // in native windows, and closes them when the page closes itself.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "CheckenLinkUIDelegate"]
        #[ivars = Popups]
        pub struct LinkUiDelegate;

        unsafe impl NSObjectProtocol for LinkUiDelegate {}

        unsafe impl WKUIDelegate for LinkUiDelegate {
            #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
            fn create_web_view(
                &self,
                _web_view: &WKWebView,
                configuration: &WKWebViewConfiguration,
                _action: &WKNavigationAction,
                _features: &WKWindowFeatures,
            ) -> Option<Retained<WKWebView>> {
                let mtm = self.mtm();
                let (window, view) = make_window(mtm, configuration, "Sign in to your bank", 560.0, 760.0);
                unsafe { view.setUIDelegate(Some(ProtocolObject::from_ref(self))) };
                window.makeKeyAndOrderFront(None);
                self.ivars().windows.borrow_mut().push(window);
                Some(view)
            }

            #[unsafe(method(webViewDidClose:))]
            fn web_view_did_close(&self, web_view: &WKWebView) {
                if let Some(w) = web_view.window() {
                    w.close();
                }
            }
        }
    );

    impl LinkUiDelegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(Popups { windows: RefCell::new(Vec::new()) });
            unsafe { msg_send![super(this), init] }
        }

        fn close_popups(&self) {
            for w in self.ivars().windows.borrow_mut().drain(..) {
                w.close();
            }
        }
    }

    fn make_window(mtm: MainThreadMarker, config: &WKWebViewConfiguration, title: &str, w: f64, h: f64) -> (Retained<NSWindow>, Retained<WKWebView>) {
        let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h));
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable;
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect, style, NSBackingStoreType::Buffered, false)
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(title));
        let view = unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), rect, config) };
        window.setContentView(Some(&view));
        window.center();
        (window, view)
    }

    /// The open Plaid Link sheet (or window).
    pub struct LinkWindow {
        window: Retained<NSWindow>,
        parent: Option<Retained<NSWindow>>,
        _view: Retained<WKWebView>,
        delegate: Retained<LinkUiDelegate>,
        closed: bool,
    }

    impl LinkWindow {
        /// Opens `url` in a sheet on Checken's window (or a centred window if
        /// there's none). Must be called on the main thread.
        pub fn open(url: &str) -> Option<LinkWindow> {
            let mtm = MainThreadMarker::new()?;
            let config = unsafe { WKWebViewConfiguration::new(mtm) };
            let (window, view) = make_window(mtm, &config, "Connect a Bank Account", 560.0, 720.0);
            let delegate = LinkUiDelegate::new(mtm);
            unsafe { view.setUIDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
            let nsurl = NSURL::URLWithString(&NSString::from_str(url))?;
            let request = NSURLRequest::requestWithURL(&nsurl);
            unsafe { view.loadRequest(&request) };
            let app = NSApplication::sharedApplication(mtm);
            let parent = app.mainWindow().or_else(|| app.keyWindow());
            match &parent {
                Some(p) => p.beginSheet_completionHandler(&window, None),
                None => window.makeKeyAndOrderFront(None),
            }
            Some(LinkWindow { window, parent, _view: view, delegate, closed: false })
        }

        /// Whether the person still has it open.
        pub fn is_open(&self) -> bool {
            !self.closed && self.window.isVisible()
        }

        pub fn close(&mut self) {
            if self.closed {
                return;
            }
            self.closed = true;
            self.delegate.close_popups();
            match &self.parent {
                Some(p) => p.endSheet(&self.window),
                None => self.window.close(),
            }
            self.window.orderOut(None);
        }
    }

    impl Drop for LinkWindow {
        fn drop(&mut self) {
            self.close();
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::LinkWindow;

/// Elsewhere Link opens in the browser; there's no window to keep.
#[cfg(not(target_os = "macos"))]
pub struct LinkWindow;

#[cfg(not(target_os = "macos"))]
impl LinkWindow {
    pub fn open(url: &str) -> Option<LinkWindow> {
        let opener = if cfg!(target_os = "windows") { "explorer" } else { "xdg-open" };
        let _ = std::process::Command::new(opener).arg(url).spawn();
        None
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn close(&mut self) {}
}
