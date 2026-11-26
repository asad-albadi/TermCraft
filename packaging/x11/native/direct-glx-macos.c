/* Qualification client for the owned XQuartz display and Apple GLX. */
#define GL_GLEXT_PROTOTYPES
#include <GL/glx.h>
#include <X11/Xlib.h>
#include <stdio.h>

int main(void) {
    Display *display = XOpenDisplay(NULL);
    if (!display) {
        fputs("Could not open the owned XQuartz display\n", stderr);
        return 1;
    }

    int attributes[] = {GLX_RGBA, GLX_RED_SIZE, 8, GLX_GREEN_SIZE, 8, GLX_BLUE_SIZE, 8, None};
    int screen = DefaultScreen(display);
    XVisualInfo *visual = glXChooseVisual(display, screen, attributes);
    if (!visual) {
        fputs("No supported GLX visual\n", stderr);
        return 1;
    }
    Window root = RootWindow(display, screen);
    XSetWindowAttributes window_attrs = {
        .colormap = XCreateColormap(display, root, visual->visual, AllocNone),
        .event_mask = StructureNotifyMask,
    };
    Window window = XCreateWindow(display, root, 24, 48, 192, 128, 0, visual->depth,
                                  InputOutput, visual->visual, CWColormap | CWEventMask,
                                  &window_attrs);
    XMapWindow(display, window);
    XEvent event;
    do {
        XNextEvent(display, &event);
    } while (event.type != MapNotify || event.xmap.window != window);

    GLXContext context = glXCreateContext(display, visual, NULL, True);
    if (!context || !glXIsDirect(display, context) || !glXMakeCurrent(display, window, context)) {
        fputs("Direct GLX context unavailable\n", stderr);
        return 1;
    }

    const GLubyte *vendor = glGetString(GL_VENDOR);
    const GLubyte *renderer = glGetString(GL_RENDERER);
    const GLubyte *version = glGetString(GL_VERSION);
    if (!vendor || !renderer || !version) {
        fputs("Direct GLX returned no renderer identity\n", stderr);
        return 1;
    }
    printf("GLX direct, vendor=%s renderer=%s version=%s\n", vendor, renderer, version);

    /* Read a framebuffer attached to the window's context. Native window
     * presentation and front-buffer visibility also depend on WindowServer. */
    GLuint framebuffer, color;
    glGenFramebuffersEXT(1, &framebuffer);
    glBindFramebufferEXT(GL_FRAMEBUFFER_EXT, framebuffer);
    glGenRenderbuffersEXT(1, &color);
    glBindRenderbufferEXT(GL_RENDERBUFFER_EXT, color);
    glRenderbufferStorageEXT(GL_RENDERBUFFER_EXT, GL_RGBA8, 192, 128);
    glFramebufferRenderbufferEXT(GL_FRAMEBUFFER_EXT, GL_COLOR_ATTACHMENT0_EXT,
                                 GL_RENDERBUFFER_EXT, color);
    if (glCheckFramebufferStatusEXT(GL_FRAMEBUFFER_EXT) != GL_FRAMEBUFFER_COMPLETE_EXT) {
        fputs("Direct GLX framebuffer is incomplete\n", stderr);
        return 1;
    }

    glViewport(0, 0, 192, 128);
    glDrawBuffer(GL_COLOR_ATTACHMENT0_EXT);
    glReadBuffer(GL_COLOR_ATTACHMENT0_EXT);
    glClearColor(1, 0, 0, 1);
    glClear(GL_COLOR_BUFFER_BIT);
    glEnable(GL_SCISSOR_TEST);
    glScissor(96, 0, 96, 128);
    glClearColor(0, 1, 0, 1);
    glClear(GL_COLOR_BUFFER_BIT);
    glFinish();

    GLubyte left[4] = {0}, right[4] = {0};
    glReadPixels(48, 64, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, left);
    glReadPixels(144, 64, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, right);
    GLenum error = glGetError();
    printf("Readback left=%u,%u,%u right=%u,%u,%u GL error=%u\n",
           left[0], left[1], left[2], right[0], right[1], right[2], error);

    glBindFramebufferEXT(GL_FRAMEBUFFER_EXT, 0);
    glDeleteFramebuffersEXT(1, &framebuffer);
    glDeleteRenderbuffersEXT(1, &color);
    glXMakeCurrent(display, None, NULL);
    glXDestroyContext(display, context);
    XDestroyWindow(display, window);
    XFreeColormap(display, window_attrs.colormap);
    XFree(visual);
    XCloseDisplay(display);
    if (error || left[0] != 255 || left[1] || left[2] || right[0] || right[1] != 255 || right[2]) {
        fputs("Direct GLX readback did not match the two drawn colors\n", stderr);
        return 1;
    }
    return 0;
}
