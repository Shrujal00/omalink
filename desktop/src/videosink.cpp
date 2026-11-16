// Puts decoded frames of the phone's screen on the Mirror page.
//
// The decoder runs on a thread of its own and hands each picture here as
// three YUV 4:2:0 planes. They're copied into a QVideoFrame, which is cheap
// to pass between threads, and given to the page's VideoOutput on the GUI
// thread. The page's VideoOutput is found by its object name, so nothing has
// to be registered with QML.
#include <QtCore/QCoreApplication>
#include <QtCore/QMetaObject>
#include <QtCore/QPointer>
#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>
#include <QtMultimedia/QVideoFrame>
#include <QtMultimedia/QVideoFrameFormat>
#include <QtMultimedia/QVideoSink>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>

#include <cstdint>
#include <cstring>

static QPointer<QVideoSink> sink;

// Items declared in QML hang off their visual parents, which isn't always
// their QObject parent, so the search follows the item tree.
static QQuickItem *findItem(QQuickItem *item, const QString &name) {
    if (!item) return nullptr;
    if (item->objectName() == name) return item;
    for (QQuickItem *child : item->childItems())
        if (QQuickItem *found = findItem(child, name)) return found;
    return nullptr;
}

static QVideoSink *findSink() {
    if (sink) return sink;
    for (QWindow *window : QGuiApplication::topLevelWindows()) {
        auto *quick = qobject_cast<QQuickWindow *>(window);
        if (!quick) continue;
        if (QQuickItem *output = findItem(quick->contentItem(), QStringLiteral("omalinkMirror"))) {
            sink = output->property("videoSink").value<QVideoSink *>();
            if (sink) return sink;
        }
    }
    return nullptr;
}

static void copyPlane(uchar *to, int toStride, const uint8_t *from, int fromStride, int bytes, int rows) {
    for (int row = 0; row < rows; ++row)
        std::memcpy(to + row * toStride, from + row * fromStride, bytes);
}

extern "C" void omalink_video_frame(const uint8_t *y, int yStride, const uint8_t *u, int uStride, const uint8_t *v, int vStride,
                                    int width, int height) {
    if (!qApp || width <= 0 || height <= 0) return;
    QVideoFrame frame(QVideoFrameFormat(QSize(width, height), QVideoFrameFormat::Format_YUV420P));
    if (!frame.map(QVideoFrame::WriteOnly)) return;
    const int halfWidth = (width + 1) / 2, halfHeight = (height + 1) / 2;
    copyPlane(frame.bits(0), frame.bytesPerLine(0), y, yStride, width, height);
    copyPlane(frame.bits(1), frame.bytesPerLine(1), u, uStride, halfWidth, halfHeight);
    copyPlane(frame.bits(2), frame.bytesPerLine(2), v, vStride, halfWidth, halfHeight);
    frame.unmap();
    QMetaObject::invokeMethod(qApp, [frame]() {
        if (QVideoSink *target = findSink()) target->setVideoFrame(frame);
    }, Qt::QueuedConnection);
}

// Blanks the page and forgets its VideoOutput, which may be a new one next time.
extern "C" void omalink_video_clear() {
    if (!qApp) return;
    QMetaObject::invokeMethod(qApp, []() {
        if (sink) sink->setVideoFrame(QVideoFrame());
        sink = nullptr;
    }, Qt::QueuedConnection);
}
