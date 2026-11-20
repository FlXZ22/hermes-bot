#include <gtk/gtk.h>
#include <webkit2/webkit2.h>
#include <stdio.h>

static int failed = 1;
static void title_changed(WebKitWebView *view, GParamSpec *unused, gpointer data) {
  const char *title = webkit_web_view_get_title(view);
  if (title && (g_str_has_prefix(title, "RESULT ") || g_str_has_prefix(title, "ERROR "))) {
    puts(title);
    failed = !g_str_has_prefix(title, "RESULT ");
    gtk_main_quit();
  }
}
static gboolean expired(gpointer data) {
  fputs("WebKit fixture timed out\n", stderr);
  gtk_main_quit();
  return G_SOURCE_REMOVE;
}
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  if (g_getenv("WAYLAND_DISPLAY") && !g_getenv("HERMES_PERF_ALLOW_DMABUF") && !g_getenv("WEBKIT_DISABLE_DMABUF_RENDERER"))
    g_setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "1", FALSE);
  if (!gtk_init_check(&argc, &argv)) {
    fputs("A desktop display is required for the WebKit performance fixture.\n", stderr);
    return 2;
  }
  GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
  gtk_window_set_default_size(GTK_WINDOW(window), 1360, 860);
  WebKitWebContext *context = webkit_web_context_new_ephemeral();
  GtkWidget *view = webkit_web_view_new_with_context(context);
  g_signal_connect(view, "notify::title", G_CALLBACK(title_changed), NULL);
  gtk_container_add(GTK_CONTAINER(window), view);
  gtk_widget_show_all(window);
  webkit_web_view_load_uri(WEBKIT_WEB_VIEW(view), argv[1]);
  g_timeout_add_seconds(30, expired, NULL);
  gtk_main();
  return failed;
}
