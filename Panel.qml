import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "ozdil.omaprot"
  ipcTarget: "ozdil.omaprot"
  manageIpc: false

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  readonly property string fontFamily: (root.bar && root.bar.fontFamily) ? root.bar.fontFamily : ((typeof Style !== "undefined" && Style.font && Style.font.family) ? Style.font.family : "JetBrainsMono Nerd Font, JetBrains Mono, monospace")

  property string activeProfileId: "bleeding-edge"
  property string activeProfileName: "Omarchy Bleeding-Edge"
  property string kernelRelease: "linux-omarchy"
  property string marchTarget: "x86-64-v3"
  property int readinessScore: 95
  property bool ntsyncReady: true
  property bool steamToolInstalled: false
  property string statusMessage: ""

  function resolveEnginePath() {
    return Qt.resolvedUrl("omaprot-engine").toString().replace(/^file:\/\//, "")
  }

  function refresh() {
    if (!probeProc.running) {
      probeProc.command = [resolveEnginePath(), "--", "--json"]
      probeProc.running = true
    }
  }

  function setProfile(profileId) {
    if (!actionProc.running) {
      actionProc.command = [resolveEnginePath(), "--", "--set-profile", profileId]
      actionProc.running = true
    }
  }

  function installSteamTool() {
    if (!actionProc.running) {
      actionProc.command = [resolveEnginePath(), "--", "--install-steam-tool"]
      actionProc.running = true
    }
  }

  Process {
    id: probeProc
    running: false
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          var data = JSON.parse(this.text)
          if (data.active_profile_id) root.activeProfileId = data.active_profile_id
          if (data.active_profile && data.active_profile.name) root.activeProfileName = data.active_profile.name
          if (data.system) {
            root.kernelRelease = data.system.kernel_release || "linux-omarchy"
            root.marchTarget = data.system.march_target || "x86-64-v3"
            root.readinessScore = data.system.readiness_score || 90
            root.ntsyncReady = Boolean(data.system.ntsync_module_present || data.system.ntsync_dev_active)
            root.steamToolInstalled = Boolean(data.system.omaprot_installed)
          }
        } catch (e) {}
      }
    }
  }

  Process {
    id: actionProc
    running: false
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          var res = JSON.parse(this.text)
          if (res.message) root.statusMessage = res.message
        } catch (e) {}
        root.refresh()
      }
    }
  }

  Component.onCompleted: root.refresh()

  Rectangle {
    id: button
    implicitWidth: rowLayout.implicitWidth + 16
    implicitHeight: 28
    radius: 6
    color: "#1a1a1a"
    border.color: "#333333"
    border.width: 1

    RowLayout {
      id: rowLayout
      anchors.centerIn: parent
      spacing: 6

      Text {
        text: "\uf135"
        font.family: root.fontFamily
        font.pixelSize: 13
        color: "#a6da95"
      }

      Text {
        text: "OmaProton [" + root.marchTarget + "]"
        font.family: root.fontFamily
        font.pixelSize: 12
        color: "#e5e5e5"
      }
    }

    MouseArea {
      anchors.fill: parent
      onClicked: root.refresh()
    }
  }
}
