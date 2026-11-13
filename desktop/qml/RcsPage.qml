import QtQuick
import omalink

// RCS chats, by way of Google's own Messages for Web.
//
// No app is allowed to read RCS conversations from the phone, so this tab
// shows the one interface that can. It runs a full web engine, which is far
// heavier than the rest of the app, so it is only loaded when the tab is
// first opened and is let go again after the tab has been idle for a while.
Item {
    id: page
    property bool current: false
    property bool wanted: false

    onCurrentChanged: {
        if (current) { wanted = true; idle.stop() }
        else if (wanted) idle.restart()
    }

    Timer { id: idle; interval: 10 * 60 * 1000; onTriggered: page.wanted = false }

    Loader {
        id: view
        anchors.fill: parent
        active: page.wanted
        asynchronous: true
        source: "RcsView.qml"
    }

    Empty {
        anchors.centerIn: parent
        visible: view.status === Loader.Loading || view.status === Loader.Null
        icon: "chat"
        title: qsTr("Opening Messages for Web…")
    }

    // The web engine is an optional package; say so rather than show nothing.
    Empty {
        anchors.centerIn: parent
        visible: view.status === Loader.Error
        icon: "chat"
        title: qsTr("The web view isn't installed")
        detail: qsTr("RCS chats are shown through Messages for Web, which needs the qt6-webengine package. Install it, then reopen Omalink.")
    }
}
