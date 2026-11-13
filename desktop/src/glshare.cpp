// The one line of C++ in the app. The embedded Messages for Web view needs
// OpenGL contexts to be shareable, and Qt only accepts that setting before
// the application object exists; the Rust bindings have no call for it.
#include <QtCore/QCoreApplication>

extern "C" void omalink_share_gl_contexts() {
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
}
