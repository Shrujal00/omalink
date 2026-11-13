import QtQuick
import QtWebEngine
import omalink

// The web view itself, kept in its own file so the web engine is only pulled
// in when RcsPage actually loads this.
Item {
    id: root
    readonly property url home: "https://messages.google.com/web/"

    WebEngineProfile {
        id: stored
        storageName: "omalink"
        offTheRecord: false
        persistentCookiesPolicy: WebEngineProfile.ForcePersistentCookies
    }

    WebEngineView {
        id: web
        anchors { left: parent.left; right: parent.right; top: bar.bottom; bottom: parent.bottom }
        url: root.home
        backgroundColor: Theme.background
        settings.javascriptCanOpenWindows: false
        settings.playbackRequiresUserGesture: true

        // A named, on-disk profile keeps the pairing with the phone between runs.
        profile: stored

        // Links in a chat open in the real browser, not in this tab.
        onNewWindowRequested: request => Qt.openUrlExternally(request.requestedUrl)
        onNavigationRequested: request => {
            const host = String(request.url).replace(/^[a-z]+:\/\//, "").split("/")[0]
            const ours = /(^|\.)google\.com$/.test(host) || /(^|\.)gstatic\.com$/.test(host)
            if (request.isMainFrame && !ours) {
                request.reject()
                Qt.openUrlExternally(request.url)
            }
        }
        // The phone's notifications already reach the desktop through
        // Omalink; a second set from the page would double every message.
        onPermissionRequested: permission => permission.deny()
    }

    // A thin strip: where you are, and a way back if the page wanders.
    Rectangle {
        id: bar
        height: 40
        anchors { left: parent.left; right: parent.right; top: parent.top }
        color: Theme.background
        Text {
            anchors { left: parent.left; leftMargin: 20; right: tools.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
            elide: Text.ElideRight
            text: web.loading ? qsTr("Loading Messages for Web…")
                : qsTr("Messages for Web  ·  Sign in or pair with your phone once; it stays connected after that")
            color: Theme.tertiary
            font.family: Theme.font; font.pixelSize: 12
        }
        Row {
            id: tools
            anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
            spacing: 2
            Btn { icon: "refresh"; iconSize: 14; kind: "ghost"; tint: Theme.secondary; implicitWidth: 30; implicitHeight: 30; hint: qsTr("Reload"); onClicked: web.url = root.home }
        }
        Rectangle { height: 1; color: Theme.line; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } }
        // A hairline of progress while the page loads.
        Rectangle {
            height: 2; color: Theme.accent
            anchors { left: parent.left; bottom: parent.bottom }
            width: parent.width * web.loadProgress / 100
            visible: web.loading
            Behavior on width { NumberAnimation { duration: Theme.quick } }
        }
    }
}
