//! macOS: `NSStatusItem` mit Menü, `UNUserNotificationCenter`, Dock-Badge, Systemklänge.
//!
//! NICHT GETESTET: Dieser Code ist unter Windows nur per `cargo check --target aarch64-apple-darwin` auf
//! Typfehler geprüft, nie gelaufen. Die Mac-Session baut und prüft ihn (Menüleiste, Rechteabfrage, Klick).
//!
//! Nur auf dem Hauptthread anlegen und benutzen (`MainThreadMarker`). Klick aufs Icon öffnet auf dem Mac das Menü
//! (wie in der Swift-App), `TrayEvent::Clicked` gibt es hier deshalb nicht; "Kadrell öffnen" ist `MenuItem("open")`.
//! `UNUserNotificationCenter` verlangt ein App-Bundle mit Bundle-Id; aus `cargo run` heraus (ohne .app) ist
//! `currentNotificationCenter` nicht benutzbar, `notify` tut dann nichts.

use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSImage, NSMenu, NSMenuItem, NSSound, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
    NSWorkspace,
};
use objc2_foundation::{NSBundle, NSDictionary, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions,
    UNNotificationRequest, UNNotificationResponse, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use crate::model::{self, ItemIcon, Notification, Sound, StatusSummary, TrayItem};
use crate::{TrayConfig, TrayEvent};

const KEY: &str = "sessionKey";

struct MenuIvars {
    events: Sender<TrayEvent>,
}

define_class!(
    /// Ziel der Menüeinträge: leitet die Auswahl als `TrayEvent::MenuItem(id)` weiter (id im `representedObject`).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KadrellTrayMenuTarget"]
    #[ivars = MenuIvars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(pick:))]
        fn pick(&self, sender: &NSMenuItem) {
            // SAFETY: `representedObject` setzen wir selbst auf einen NSString.
            let id = sender.representedObject()
                .and_then(|o| o.downcast::<NSString>().ok())
                .map(|s| s.to_string());
            if let Some(id) = id {
                let _ = self.ivars().events.send(TrayEvent::MenuItem(id));
            }
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}
);

struct DelegateIvars {
    events: Sender<TrayEvent>,
}

define_class!(
    /// Delegate des Benachrichtigungscenters: Klick liefert den Session-Schlüssel, Banner auch bei aktiver App.
    #[unsafe(super(NSObject))]
    #[name = "KadrellTrayNotificationDelegate"]
    #[ivars = DelegateIvars]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            done: &block2::DynBlock<dyn Fn()>,
        ) {
            let info = response.notification().request().content().userInfo();
            let key = info
                .objectForKey(&NSString::from_str(KEY))
                .and_then(|o| o.downcast::<NSString>().ok())
                .map(|s| s.to_string());
            if let Some(key) = key {
                let _ = self.ivars().events.send(TrayEvent::NotificationClicked(key));
            }
            done.call(());
        }

        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            done: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            done.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::Sound,));
        }
    }
);

/// Das Mitteilungscenter, nur wenn der Prozess ein App-Bündel ist: `currentNotificationCenter` wirft sonst eine
/// Ausnahme (`cargo run`, Tests).
fn notification_center() -> Option<Retained<UNUserNotificationCenter>> {
    let bundle = NSBundle::mainBundle();
    let in_app = bundle.bundleIdentifier().is_some() && bundle.bundlePath().to_string().ends_with(".app");
    in_app.then(UNUserNotificationCenter::currentNotificationCenter)
}

pub struct Backend {
    mtm: MainThreadMarker,
    item: Retained<NSStatusItem>,
    target: Retained<MenuTarget>,
    delegate: Retained<NotificationDelegate>,
    authorized: std::cell::Cell<bool>,
}

impl Backend {
    /// Paniert, wenn nicht auf dem Hauptthread aufgerufen (AppKit verlangt es).
    pub fn new(_config: TrayConfig, events: Sender<TrayEvent>) -> Self {
        let mtm = MainThreadMarker::new().expect("kadrell-tray: Tray::new nur auf dem Hauptthread");
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        let target = MenuTarget::alloc(mtm).set_ivars(MenuIvars { events: events.clone() });
        let target: Retained<MenuTarget> = unsafe { msg_send![super(target), init] };
        let delegate = NotificationDelegate::alloc().set_ivars(DelegateIvars { events });
        let delegate: Retained<NotificationDelegate> = unsafe { msg_send![super(delegate), init] };
        if let Some(button) = item.button(mtm) {
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("terminal"), Some(&NSString::from_str("Kadrell")));
            if let Some(image) = image {
                image.setTemplate(true);
                button.setImage(Some(&image));
            }
        }
        let backend = Backend { mtm, item, target, delegate, authorized: false.into() };
        backend.set_menu(model::build_menu(&[], &[], model::Lang::default()));
        backend.start_notifications();
        backend
    }

    pub fn set_status(&self, s: StatusSummary) {
        self.item.setVisible(s.display != model::Display::Hidden);
        if let Some(button) = self.item.button(self.mtm) {
            button.setTitle(&NSString::from_str(&model::title_for(&s)));
        }
    }

    pub fn set_menu(&self, items: Vec<TrayItem>) {
        let menu = NSMenu::new(self.mtm);
        for item in &items {
            match item {
                TrayItem::Separator => menu.addItem(&NSMenuItem::separatorItem(self.mtm)),
                TrayItem::Entry { id, label, icon, enabled } => {
                    menu.addItem(&self.menu_item(id, label, *icon, *enabled));
                }
            }
        }
        self.item.setMenu(Some(&menu));
    }

    fn menu_item(&self, id: &str, label: &str, icon: ItemIcon, enabled: bool) -> Retained<NSMenuItem> {
        // SAFETY: `pick:` ist auf `MenuTarget` definiert und nimmt den NSMenuItem.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.mtm),
                &NSString::from_str(label),
                Some(sel!(pick:)),
                &NSString::new(),
            )
        };
        // SAFETY: Zielobjekt lebt, solange `Backend` lebt; das Menü hält den Eintrag nur während es offen ist.
        unsafe {
            item.setTarget(Some(&self.target));
            item.setRepresentedObject(Some(&NSString::from_str(id)));
        }
        item.setEnabled(enabled);
        let symbol = match icon {
            ItemIcon::None => None,
            ItemIcon::Waiting => Some("clock"),
            ItemIcon::Done => Some("checkmark.circle"),
        };
        if let Some(name) = symbol {
            item.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None).as_deref());
        }
        item
    }

    /// Delegate setzen und die Berechtigung gleich beim Start abfragen, nicht erst bei der ersten Mitteilung: dann
    /// kommt die Abfrage, solange der Nutzer vor der App sitzt, und ein Klick auf eine Mitteilung erreicht die App
    /// auch dann, wenn sie dadurch gestartet wurde.
    fn start_notifications(&self) {
        let Some(center) = notification_center() else { return };
        center.setDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
        if !self.authorized.replace(true) {
            let options = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge;
            center.requestAuthorizationWithOptions_completionHandler(options, &RcBlock::new(|_granted, _err: *mut NSError| {}));
        }
    }

    pub fn notify(&self, n: Notification) {
        let Some(center) = notification_center() else { return };
        // SAFETY: `userInfo` ist ein NSDictionary aus NSString-Paaren, wie es UserNotifications erwartet.
        unsafe {
            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(&n.title));
            content.setBody(&NSString::from_str(&n.body));
            content.setThreadIdentifier(&NSString::from_str(&n.group));
            let info = NSDictionary::from_retained_objects(&[&*NSString::from_str(KEY)], &[Retained::<AnyObject>::from(NSString::from_str(&n.session_key))]);
            content.setUserInfo(info.cast_unchecked::<AnyObject, AnyObject>());
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&n.session_key), &content, None);
            center.addNotificationRequest_withCompletionHandler(&request, None);
        }
    }

    pub fn set_badge(&self, count: usize) {
        let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
        NSApplication::sharedApplication(self.mtm).dockTile().setBadgeLabel(label.as_deref());
    }

    pub fn set_window_handle(&self, _hwnd: isize) {}
}

impl Drop for Backend {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

pub fn play(sound: Sound) {
    // Kopie: eine laufende NSSound-Instanz spielt nicht ein zweites Mal gleichzeitig (wie in Swift).
    let named = NSSound::soundNamed(&NSString::from_str(sound.mac_name()));
    // SAFETY: `copy` liefert wieder einen NSSound.
    let copy: Option<Retained<NSSound>> = named.and_then(|s| unsafe { msg_send![&*s, copy] });
    if let Some(s) = copy {
        s.setVolume(0.3);
        s.play();
    }
}

pub fn beep() {
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        fn NSBeep();
    }
    // SAFETY: `NSBeep` hat keine Vorbedingungen und darf vom Hauptthread aufgerufen werden.
    unsafe { NSBeep() }
}

pub fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}
