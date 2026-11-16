import QtQuick
import omalink

// A surface that lifts a little under the pointer. Pages build on it.
Rectangle {
    id: card
    property bool interactive: true
    // Off when the card's content handles its own clicks (or drags).
    property bool tappable: interactive
    readonly property alias hovered: hover.hovered
    default property alias content: body.data
    signal clicked()

    radius: Theme.card
    color: hover.hovered && interactive ? Theme.raised : Theme.surface
    border.width: activeFocus ? 1.5 : 1
    border.color: activeFocus ? Theme.accent : Theme.line
    transform: Translate {
        y: hover.hovered && card.interactive ? -2 : 0
        Behavior on y { NumberAnimation { duration: Theme.quick; easing.type: Easing.OutCubic } }
    }
    Behavior on color { ColorAnimation { duration: Theme.quick } }

    HoverHandler { id: hover; cursorShape: card.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor }
    TapHandler { enabled: card.tappable; onTapped: card.clicked() }
    Item { id: body; anchors.fill: parent }
}
