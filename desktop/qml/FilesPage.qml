import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtCore
import omalink

// Files to and from the phone, as a gallery: pictures and videos show
// themselves, everything else its kind. Drop files anywhere to send them,
// paste a copied file or image with Ctrl+V, drag a card out to keep it.
Item {
    id: page
    property bool current: false
    property string view: "grid"
    // "shared": what went back and forth; "phone": the phone's own gallery.
    property string tab: "shared"
    // Gallery items picked for downloading or deleting, by id.
    property var picked: ({})
    readonly property int pickedCount: Object.keys(picked).length
    signal setup()

    function togglePick(id) {
        const next = Object.assign({}, picked)
        if (next[id]) delete next[id]; else next[id] = true
        picked = next
    }
    function pickedIds() { return Object.keys(picked).map(Number) }
    function day(ms) {
        const d = new Date(ms), now = new Date()
        const start = t => new Date(t.getFullYear(), t.getMonth(), t.getDate()).getTime()
        const days = Math.round((start(now) - start(d)) / 86400000)
        if (days === 0) return qsTr("Today")
        if (days === 1) return qsTr("Yesterday")
        if (days < 7) return Qt.locale().dayName(d.getDay())
        return d.toLocaleDateString(Qt.locale(), d.getFullYear() === now.getFullYear() ? "d MMMM" : "d MMMM yyyy")
    }
    // The gallery, a group a day.
    readonly property var days: {
        const out = []
        for (const m of Store.gallery.items) {
            const label = page.day(m.time)
            if (!out.length || out[out.length - 1].label !== label) out.push({ label: label, items: [] })
            out[out.length - 1].items.push(m)
        }
        return out
    }
    function openGallery() {
        tab = "phone"
        if (Store.connected && Store.has("photos") && Store.gallery.items.length === 0 && !Store.gallery.loading) Store.send("gallery_load", { more: false })
    }

    // Remembered between runs, beside the rest of the config.
    Settings {
        location: StandardPaths.writableLocation(StandardPaths.ConfigLocation) + "/omalink/ui.conf"
        category: "files"
        property alias view: page.view
        property alias tab: page.tab
    }

    readonly property var moving: Store.transfers.filter(t => t.state === "active" || t.state === "waiting")
    readonly property var finished: Store.transfers.filter(t => t.state !== "active" && t.state !== "waiting")
    readonly property string phone: Store.link.name || qsTr("your phone")

    function send(urls) {
        const paths = []
        for (const url of urls) {
            const text = String(url)
            if (text.startsWith("file://")) paths.push(decodeURIComponent(text.slice(7)))
        }
        if (paths.length) Store.send("send_files", { paths: paths })
    }
    function choose() { picker.open() }

    function size(bytes) {
        if (bytes < 0) return ""
        if (bytes < 1024) return qsTr("%1 B").arg(bytes)
        if (bytes < 1024 * 1024) return qsTr("%1 KB").arg(Math.round(bytes / 1024))
        if (bytes < 1024 * 1024 * 1024) return qsTr("%1 MB").arg((bytes / 1048576).toFixed(1))
        return qsTr("%1 GB").arg((bytes / 1073741824).toFixed(2))
    }
    function timeLeft(seconds) {
        if (seconds < 0) return ""
        if (seconds < 60) return qsTr("%1 s left").arg(Math.max(1, seconds))
        if (seconds < 3600) return qsTr("%1 min left").arg(Math.round(seconds / 60))
        return qsTr("%1 h left").arg((seconds / 3600).toFixed(1))
    }
    // What a moving file says under its name.
    function progress(t) {
        if (t.state === "waiting") return qsTr("Waiting for the phone…")
        const parts = [page.size(t.done) + (t.size > 0 ? " / " + page.size(t.size) : "")]
        if (t.speed > 0) parts.push(page.size(Math.round(t.speed)) + "/s")
        if (t.eta >= 0) parts.push(page.timeLeft(t.eta))
        return parts.join("  ·  ")
    }
    function detail(t) {
        if (t.state === "failed") return t.error || qsTr("Didn't arrive")
        const parts = [page.size(t.size), Store.when(t.time)]
        if (t.via === "network" && Store.net.route === "relay") parts.push(qsTr("via relay"))
        return parts.filter(p => p !== "").join("  ·  ")
    }
    function fraction(t) { return t.state === "waiting" || t.size <= 0 ? -1 : t.done / t.size }
    function openable(t) { return t.state === "done" }

    Connections { target: Store; function onDemoTab(tab) { page.tab = tab } }

    Shortcut {
        sequences: [StandardKey.Paste]
        enabled: page.current && Store.connected
        onActivated: Store.send("paste_send")
    }

    FileDialog {
        id: picker
        title: qsTr("Send to phone")
        fileMode: FileDialog.OpenFiles
        onAccepted: page.send(selectedFiles)
    }

    Gate { id: gate; anchors.fill: parent; icon: "file"; what: qsTr("Files"); onSetup: page.setup() }

    Flickable {
        id: scroller
        anchors.fill: parent
        visible: !gate.blocked
        contentHeight: column.height + Theme.s6 * 2
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
        // Near the end of the gallery, fetch the next page.
        onContentYChanged: {
            if (page.tab === "phone" && Store.gallery.more && !Store.gallery.loading && contentY + height > contentHeight - 600)
                Store.send("gallery_load", { more: true })
        }

        Column {
            id: column
            readonly property real margin: Theme.margin(page.width)
            width: Math.min(page.width - margin * 2, 1640)
            anchors.horizontalCenter: parent.horizontalCenter
            y: Theme.s5
            spacing: Theme.s5

            PageHeader {
                width: parent.width
                title: ""

                // Shared files, or the phone's gallery, as two big tabs.
                lead: Row {
                    spacing: Theme.s5
                    Repeater {
                        model: [["shared", qsTr("Shared")], ["phone", qsTr("Phone")]]
                        Item {
                            required property var modelData
                            readonly property bool on: page.tab === modelData[0]
                            width: label.implicitWidth; height: 44
                            Text {
                                id: label
                                text: modelData[1]
                                color: parent.on ? Theme.foreground : tabHover.hovered ? Theme.secondary : Theme.tertiary
                                font.family: Theme.font; font.pixelSize: Theme.display; font.weight: Font.DemiBold
                                Behavior on color { ColorAnimation { duration: Theme.quick } }
                            }
                            Rectangle {
                                anchors { left: label.left; bottom: parent.bottom }
                                height: 3; radius: 1.5; color: Theme.accent
                                width: parent.on ? label.implicitWidth : 0
                                Behavior on width { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } }
                            }
                            HoverHandler { id: tabHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: modelData[0] === "phone" ? page.openGallery() : page.tab = "shared" }
                        }
                    }
                }

                // The gallery's own actions.
                Row {
                    visible: page.tab === "phone"
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Theme.s2
                    Text {
                        visible: page.pickedCount > 0
                        anchors.verticalCenter: parent.verticalCenter
                        rightPadding: Theme.s2
                        text: page.pickedCount
                        color: Theme.accent
                        font.family: Theme.font; font.pixelSize: Theme.title; font.weight: Font.DemiBold
                    }
                    Btn { visible: page.pickedCount > 0; icon: "x"; iconSize: 15; kind: "ghost"; tint: Theme.secondary; implicitWidth: 38; hint: qsTr("Clear selection")
                          anchors.verticalCenter: parent.verticalCenter; onClicked: page.picked = ({}) }
                    Btn { visible: page.pickedCount > 0; icon: "trash"; iconSize: 16; kind: "soft"; tint: Theme.red; implicitWidth: 42; implicitHeight: 38
                          hint: qsTr("Delete from the phone"); anchors.verticalCenter: parent.verticalCenter
                          onClicked: { Store.send("gallery_delete", { ids: page.pickedIds() }); page.picked = ({}) } }
                    Btn { visible: page.pickedCount > 0; icon: "in"; iconSize: 18; kind: "primary"; implicitWidth: 46; implicitHeight: 38
                          hint: qsTr("Download to this computer"); anchors.verticalCenter: parent.verticalCenter
                          onClicked: { Store.send("gallery_get", { ids: page.pickedIds() }); page.picked = ({}) } }
                    Btn { visible: page.pickedCount === 0; icon: "refresh"; iconSize: 16; kind: "ghost"; tint: Theme.secondary; implicitWidth: 38
                          hint: qsTr("Refresh"); anchors.verticalCenter: parent.verticalCenter; onClicked: Store.send("gallery_load", { more: false }) }
                }

                // Grid or list.
                Rectangle {
                    visible: page.tab === "shared"
                    width: toggle.width + 6; height: 34; radius: Theme.small
                    color: Theme.surface
                    anchors.verticalCenter: parent.verticalCenter
                    Row {
                        id: toggle
                        anchors.centerIn: parent
                        spacing: 2
                        Repeater {
                            model: [["grid", qsTr("Grid")], ["list", qsTr("List")]]
                            Btn {
                                required property var modelData
                                icon: modelData[0]; hint: modelData[1]; iconSize: 15
                                implicitWidth: 34; implicitHeight: 28
                                kind: page.view === modelData[0] ? "soft" : "ghost"
                                tint: page.view === modelData[0] ? Theme.foreground : Theme.secondary
                                onClicked: page.view = modelData[0]
                            }
                        }
                    }
                }
                Btn { visible: page.tab === "shared" && page.finished.length > 0; icon: "trash"; iconSize: 16; kind: "ghost"; tint: Theme.secondary
                      implicitWidth: 38; hint: qsTr("Clear finished")
                      anchors.verticalCenter: parent.verticalCenter; onClicked: Store.send("clear_transfers") }
                Btn { visible: page.tab === "shared"; icon: "upload"; iconSize: 18; kind: "primary"; implicitWidth: 46; implicitHeight: 38
                      hint: qsTr("Send files  ·  or drop them anywhere, or press Ctrl+V")
                      anchors.verticalCenter: parent.verticalCenter; onClicked: page.choose() }
            }

            Empty {
                visible: page.tab === "shared" && Store.transfers.length === 0
                anchors.horizontalCenter: parent.horizontalCenter
                topPadding: Theme.s6
                icon: "image"
                title: qsTr("Drop something here")
                action: qsTr("Choose files")
                onTriggered: page.choose()
            }

            FileGroup { visible: page.tab === "shared" && items.length > 0; label: qsTr("Moving"); items: page.moving }
            FileGroup { visible: page.tab === "shared" && items.length > 0; label: qsTr("Recent"); items: page.finished }

            // The phone's gallery.
            Empty {
                visible: page.tab === "phone" && !Store.has("photos")
                anchors.horizontalCenter: parent.horizontalCenter
                topPadding: Theme.s6
                icon: "image"
                title: qsTr("Allow photos on your phone")
                action: qsTr("Ask on phone")
                onTriggered: Store.send("request_permissions", { permissions: ["photos"] })
            }
            Empty {
                visible: page.tab === "phone" && Store.has("photos") && Store.gallery.items.length === 0 && !Store.gallery.loading
                anchors.horizontalCenter: parent.horizontalCenter
                topPadding: Theme.s6
                icon: "image"
                title: qsTr("No photos yet")
            }
            Repeater {
                model: page.tab === "phone" ? page.days : []
                Column {
                    id: dayGroup
                    required property var modelData
                    width: column.width
                    spacing: Theme.s3
                    Text {
                        text: dayGroup.modelData.label
                        color: Theme.secondary
                        font.family: Theme.font; font.pixelSize: Theme.caption; font.weight: Font.DemiBold
                        font.capitalization: Font.AllUppercase; font.letterSpacing: 0.6
                    }
                    Flow {
                        id: tiles
                        width: parent.width
                        spacing: 6
                        readonly property int columns: Math.max(3, Math.floor((width + spacing) / (150 + spacing)))
                        readonly property real cell: (width - spacing * (columns - 1)) / columns
                        Repeater {
                            model: dayGroup.modelData.items
                            GalleryTile { width: tiles.cell; height: tiles.cell }
                        }
                    }
                }
            }
            ProgressRing {
                visible: page.tab === "phone" && Store.gallery.loading
                anchors.horizontalCenter: parent.horizontalCenter
                value: -1
            }
        }
    }

    // One titled group of files, as a grid or a list.
    component FileGroup: Column {
        id: section
        property string label: ""
        property var items: []
        visible: items.length > 0
        width: column.width
        spacing: Theme.s3

        Text {
            text: section.label + "  " + section.items.length
            color: Theme.secondary
            font.family: Theme.font; font.pixelSize: Theme.caption; font.weight: Font.DemiBold
            font.capitalization: Font.AllUppercase; font.letterSpacing: 0.6
        }

        Flow {
            id: grid
            visible: page.view === "grid"
            width: parent.width
            spacing: Theme.s4
            readonly property int columns: Math.max(1, Math.floor((width + spacing) / (220 + spacing)))
            readonly property real cell: (width - spacing * (columns - 1)) / columns
            Repeater {
                model: page.view === "grid" ? section.items : []
                FileCard { width: grid.cell }
            }
        }

        Column {
            visible: page.view === "list"
            width: parent.width
            spacing: Theme.s2
            Repeater {
                model: page.view === "list" ? section.items : []
                FileRow { width: section.width }
            }
        }
    }

    // A file as a picture with its name under it.
    component FileCard: Card {
        id: card
        required property var modelData
        readonly property var t: modelData
        readonly property bool moving: t.state === "active" || t.state === "waiting"
        height: picture.height + meta.height + 20
        tappable: false

        Thumb {
            id: picture
            x: 6; y: 6
            width: card.width - 12; height: Math.round(width * 0.62)
            radius: Math.max(0, Theme.card - 3)
            source: card.t.thumb
            kind: card.t.kind
            glyphSize: 38

            // Which way it went, as an arrow.
            Rectangle {
                x: 8; y: 8
                width: 26; height: 26; radius: 13
                color: picture.loaded ? Qt.rgba(0, 0, 0, 0.55) : Theme.surface
                Icon {
                    anchors.centerIn: parent
                    name: card.t.state === "failed" ? "x" : card.t.incoming ? "in" : "out"
                    size: 14; weight: 2
                    color: card.t.state === "failed" ? Theme.red : card.t.incoming ? Theme.accent : "white"
                }
                HoverHandler { id: wayHover }
                Tip { visible: wayHover.hovered; text: card.t.incoming ? qsTr("From your phone") : qsTr("To your phone") }
            }

            // While it moves, the picture dims under a ring.
            Rectangle {
                anchors.fill: parent
                visible: card.moving
                color: Qt.rgba(0, 0, 0, 0.45)
                radius: parent.radius
                ProgressRing {
                    anchors.centerIn: parent
                    width: 46; height: 46; thickness: 4
                    value: page.fraction(card.t)
                }
            }

            // Actions on hover, once it's here.
            Row {
                anchors { right: parent.right; bottom: parent.bottom; margins: 8 }
                spacing: 6
                visible: card.hovered && page.openable(card.t)
                opacity: visible ? 1 : 0
                Btn { icon: "folder"; iconSize: 15; kind: "primary"; implicitWidth: 34; implicitHeight: 32; hint: qsTr("Show in folder")
                      onClicked: Store.send("show_path", { path: card.t.path }) }
            }
        }

        Column {
            id: meta
            anchors { left: parent.left; right: parent.right; top: picture.bottom; margins: 12; topMargin: 10 }
            spacing: 3
            Text {
                width: parent.width; elide: Text.ElideMiddle
                text: card.t.name
                textFormat: Text.PlainText
                color: Theme.foreground
                font.family: Theme.font; font.pixelSize: Theme.label; font.weight: Font.Medium
            }
            Text {
                width: parent.width; elide: Text.ElideRight
                text: card.moving ? page.progress(card.t) : page.detail(card.t)
                color: card.t.state === "failed" ? Theme.red : Theme.tertiary
                font.family: Theme.font; font.pixelSize: Theme.caption
            }
        }

        DragOut { anchors.fill: parent; t: card.t; image: picture.source; onOpen: Store.send("open_path", { path: card.t.path }) }
    }

    // A file as one line, for long lists.
    component FileRow: Card {
        id: row
        required property var modelData
        readonly property var t: modelData
        readonly property bool moving: t.state === "active" || t.state === "waiting"
        height: 64
        tappable: false

        Thumb {
            id: small
            anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter }
            width: 44; height: 44; radius: Theme.small
            source: row.t.thumb; kind: row.t.kind; glyphSize: 20
        }
        Column {
            anchors { left: small.right; leftMargin: 12; right: side.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
            spacing: 4
            Text {
                width: parent.width; elide: Text.ElideMiddle
                text: row.t.name; textFormat: Text.PlainText
                color: Theme.foreground
                font.family: Theme.font; font.pixelSize: Theme.label; font.weight: Font.Medium
            }
            Text {
                width: parent.width; elide: Text.ElideRight
                text: (row.t.incoming ? "↙  " : "↗  ") + (row.moving ? page.progress(row.t) : page.detail(row.t))
                color: row.t.state === "failed" ? Theme.red : Theme.tertiary
                font.family: Theme.font; font.pixelSize: Theme.caption
            }
        }
        Row {
            id: side
            anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
            spacing: 4
            ProgressRing { visible: row.moving; width: 28; height: 28; value: page.fraction(row.t); anchors.verticalCenter: parent.verticalCenter }
            Btn { visible: page.openable(row.t); icon: "folder"; iconSize: 14; kind: "ghost"; tint: Theme.secondary; implicitWidth: 30; implicitHeight: 30
                  hint: qsTr("Show in folder"); onClicked: Store.send("show_path", { path: row.t.path }) }
        }
        DragOut { anchors { fill: parent; rightMargin: side.width + 12 } t: row.t; image: small.source; onOpen: Store.send("open_path", { path: row.t.path }) }
    }

    // Drag a finished file out of the window, into a file manager or another
    // app. A click opens it.
    component DragOut: MouseArea {
        id: area
        property var t: null
        property string image: ""
        signal open()
        enabled: t !== null && t.state === "done"
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        drag.target: proxy
        onClicked: area.open()

        Item {
            id: proxy
            Drag.active: area.drag.active
            Drag.dragType: Drag.Automatic
            Drag.supportedActions: Qt.CopyAction
            Drag.mimeData: area.t ? { "text/uri-list": "file://" + encodeURI(area.t.path) } : {}
            Drag.imageSource: area.image
        }
    }

    // One photo or video from the phone. Click picks it; double-click
    // downloads it straight away.
    component GalleryTile: Item {
        id: tile
        required property var modelData
        readonly property bool on: !!page.picked[modelData.id]

        HoverHandler { id: tileHover; cursorShape: Qt.PointingHandCursor }
        TapHandler {
            onTapped: page.togglePick(tile.modelData.id)
            onDoubleTapped: Store.send("gallery_get", { ids: [tile.modelData.id] })
        }

        Thumb {
            anchors.fill: parent
            anchors.margins: tile.on ? 6 : 0
            radius: Theme.small
            source: tile.modelData.thumb
            kind: tile.modelData.kind
            glyphSize: 26
            Behavior on anchors.margins { NumberAnimation { duration: Theme.quick; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.small
            color: "transparent"
            border.width: tile.on ? 2 : 0
            border.color: Theme.accent
        }
        // How long a video runs.
        Rectangle {
            visible: tile.modelData.kind === "video" && tile.modelData.duration_ms > 0
            anchors { right: parent.right; bottom: parent.bottom; margins: 6 }
            width: length.implicitWidth + 10; height: 18; radius: 9
            color: Qt.rgba(0, 0, 0, 0.6)
            Text {
                id: length
                anchors.centerIn: parent
                text: {
                    const s = Math.round(tile.modelData.duration_ms / 1000)
                    return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0")
                }
                color: "white"; font.family: Theme.font; font.pixelSize: 10; font.weight: Font.DemiBold
            }
        }
        // The pick mark.
        Rectangle {
            visible: tile.on || tileHover.hovered || page.pickedCount > 0
            anchors { left: parent.left; top: parent.top; margins: 8 }
            width: 22; height: 22; radius: 11
            color: tile.on ? Theme.accent : Qt.rgba(0, 0, 0, 0.45)
            border.width: 2; border.color: tile.on ? Theme.accent : "white"
            Icon { visible: tile.on; anchors.centerIn: parent; name: "check"; size: 13; weight: 2.4; color: Theme.onAccent }
        }
        // Quick download on hover.
        Btn {
            visible: tileHover.hovered && page.pickedCount === 0
            anchors { right: parent.right; top: parent.top; margins: 6 }
            icon: "in"; iconSize: 14; kind: "primary"; implicitWidth: 30; implicitHeight: 30
            hint: qsTr("Download")
            onClicked: Store.send("gallery_get", { ids: [tile.modelData.id] })
        }
    }
}
