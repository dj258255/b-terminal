//! b-terminal(#625): desktop notifications through `UNUserNotificationCenter`, so that clicking one can bring the app to
//! the terminal pane that sent it (as the Ghostty macOS app does). winit 0.30 has no notification API; this module is
//! the only place that touches the framework. The app sees safe functions in `platform::macos`.
//!
//! `UNUserNotificationCenter` raises an Objective-C exception (which aborts) in a process without a bundle identifier,
//! so every entry point checks [`supported`] first and does nothing otherwise. The app keeps its old path for that case.

use std::sync::{Arc, Mutex, Once};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
use objc2_foundation::{NSArray, NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationDefaultActionIdentifier, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use crate::platform::macos::UserNotificationResponse;

type Handler = Arc<dyn Fn(UserNotificationResponse) + Send + Sync>;

static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);
static DELEGATE: Once = Once::new();

/// Whether the process has a bundle identifier (it runs from an app bundle). Without one the framework cannot be used.
pub fn supported() -> bool {
    // SAFETY: `bundleIdentifier` has no preconditions; `mainBundle` always returns a bundle object.
    unsafe { NSBundle::mainBundle().bundleIdentifier() }.is_some()
}

fn center() -> Retained<UNUserNotificationCenter> {
    // SAFETY: only called after `supported()` returned true (see the module comment).
    unsafe { UNUserNotificationCenter::currentNotificationCenter() }
}

declare_class!(
    struct NotificationDelegate;

    unsafe impl ClassType for NotificationDelegate {
        type Super = NSObject;
        // The framework may call the delegate on a background queue; it holds no state.
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BTUserNotificationDelegate";
    }

    impl DeclaredClass for NotificationDelegate {}

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        /// The app posts only for panes the user is not looking at, so a notification that arrives while the app is in
        /// front is still shown (Ghostty decides per pane here; the app made that decision before posting).
        #[method(userNotificationCenter:willPresentNotification:withCompletionHandler:)]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::Block<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler.call((UNNotificationPresentationOptions::UNNotificationPresentationOptionBanner
                | UNNotificationPresentationOptions::UNNotificationPresentationOptionList
                | UNNotificationPresentationOptions::UNNotificationPresentationOptionSound,));
        }

        #[method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:)]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::Block<dyn Fn()>,
        ) {
            // SAFETY: plain property reads on a live response object.
            let (id, action) = unsafe {
                (response.notification().request().identifier().to_string(), response.actionIdentifier())
            };
            // SAFETY: a framework constant, valid for the life of the process.
            let clicked = action.as_ref() == unsafe { UNNotificationDefaultActionIdentifier };
            deliver(UserNotificationResponse { id, clicked });
            completion_handler.call(());
        }
    }
);

impl NotificationDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: `NSObject`'s designated initialiser.
        unsafe { msg_send_id![super(this), init] }
    }
}

fn deliver(response: UserNotificationResponse) {
    let handler = HANDLER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(handler) = handler {
        handler(response);
    }
}

/// Registers the function that receives clicks, installs the delegate and clears the notifications an earlier run
/// left in Notification Center (their panes are gone). Does nothing but store the handler without a bundle.
pub fn set_handler(handler: Box<dyn Fn(UserNotificationResponse) + Send + Sync>) {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(handler.into());
    if !supported() {
        return;
    }
    DELEGATE.call_once(|| {
        let center = center();
        let delegate = NotificationDelegate::new();
        // SAFETY: the delegate property is weak; the delegate is leaked below so it outlives the center's use of it.
        unsafe { center.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
        std::mem::forget(delegate);
        // SAFETY: no preconditions.
        unsafe { center.removeAllDeliveredNotifications() };
    });
}

/// Posts a notification with the given identifier. Asks for permission first (the system asks the user once). Returns
/// `false` without a bundle; delivery itself is asynchronous and failures are only logged.
pub fn post(id: &str, title: &str, body: &str) -> bool {
    if !supported() {
        return false;
    }
    // SAFETY: setters on a freshly created mutable content object; strings are copied by the framework.
    let request = unsafe {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(id), &content, None)
    };
    let add = RcBlock::new(move |granted: Bool, error: *mut NSError| {
        if !error.is_null() {
            // SAFETY: the framework passes a valid error object or null.
            tracing::warn!("notification permission request failed: {}", unsafe { &*error }.localizedDescription());
        }
        if !granted.as_bool() {
            return;
        }
        let logged = RcBlock::new(|error: *mut NSError| {
            if !error.is_null() {
                // SAFETY: as above.
                tracing::warn!("posting a notification failed: {}", unsafe { &*error }.localizedDescription());
            }
        });
        // SAFETY: `request` is a valid request kept alive by this block; the completion block may run on any thread.
        unsafe { center().addNotificationRequest_withCompletionHandler(&request, Some(&logged)) };
    });
    let options = UNAuthorizationOptions::UNAuthorizationOptionAlert | UNAuthorizationOptions::UNAuthorizationOptionSound;
    // SAFETY: the block is retained by the framework until it has been called.
    unsafe { center().requestAuthorizationWithOptions_completionHandler(options, &add) };
    true
}

/// Removes delivered notifications by identifier (a pane was looked at or closed). Does nothing without a bundle.
pub fn remove(ids: &[String]) {
    if ids.is_empty() || !supported() {
        return;
    }
    let ids: Vec<Retained<NSString>> = ids.iter().map(|s| NSString::from_str(s)).collect();
    let ids = NSArray::from_vec(ids);
    // SAFETY: an array of strings, as the method expects.
    unsafe { center().removeDeliveredNotificationsWithIdentifiers(&ids) };
}

/// Removes every delivered notification of this app (the app is quitting). Does nothing without a bundle.
pub fn remove_all() {
    if supported() {
        // SAFETY: no preconditions.
        unsafe { center().removeAllDeliveredNotifications() };
    }
}
