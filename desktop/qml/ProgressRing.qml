import QtQuick
import QtQuick.Shapes
import omalink

// How far along something is, as a ring. A negative value spins: the end
// isn't known yet.
Item {
    id: ring
    property real value: 0
    property real thickness: 3
    property color color: Theme.accent
    implicitWidth: 34
    implicitHeight: 34

    readonly property bool unknown: value < 0
    property real shown: unknown ? 0.25 : Math.max(0.02, Math.min(1, value))
    Behavior on shown { enabled: !ring.unknown; NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } }

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        rotation: ring.unknown ? spin.angle : 0
        ShapePath {
            fillColor: "transparent"
            strokeColor: Theme.pressed
            strokeWidth: ring.thickness
            PathAngleArc { centerX: ring.width / 2; centerY: ring.height / 2; radiusX: ring.width / 2 - ring.thickness; radiusY: radiusX; startAngle: 0; sweepAngle: 360 }
        }
        ShapePath {
            fillColor: "transparent"
            strokeColor: ring.color
            strokeWidth: ring.thickness
            capStyle: ShapePath.RoundCap
            PathAngleArc { centerX: ring.width / 2; centerY: ring.height / 2; radiusX: ring.width / 2 - ring.thickness; radiusY: radiusX; startAngle: -90; sweepAngle: 360 * ring.shown }
        }
    }
    QtObject { id: spin; property real angle: 0 }
    NumberAnimation { target: spin; property: "angle"; from: 0; to: 360; duration: 1100; loops: Animation.Infinite; running: ring.unknown && ring.visible }
}
