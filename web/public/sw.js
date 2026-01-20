self.addEventListener("push", (event) => {
  const data = (() => {
    try {
      return event.data ? event.data.json() : null;
    } catch {
      return event.data ? { body: event.data.text() } : null;
    }
  })();

  const title = data?.title || "catnap";
  const body = data?.body || "";
  const url = data?.url || "/";

  event.waitUntil(
    self.registration.showNotification(title, {
      body,
      data: { url },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = event.notification?.data?.url || "/";
  event.waitUntil(self.clients.openWindow(url));
});
