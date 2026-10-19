import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import "theme"

Rectangle {
    id: root
    color: Theme.bgBase

    readonly property string fontFamily: (typeof Style !== "undefined" && Style.font && Style.font.family) ? Style.font.family : Theme.fontFamily

    property string activeProfileId: "bleeding-edge"
    property string activeProfileName: "Omarchy Bleeding-Edge"
    property string activeTagline: ""
    property string kernelRelease: "--"
    property bool boreActive: true
    property bool adiosActive: true
    property bool ntsyncMod: true
    property bool ntsyncDev: false
    property string cpuModel: "--"
    property string isaLevel: "--"
    property string marchTarget: "--"
    property string cflagsText: "--"
    property bool steamInstalled: false
    property string steamInstallPath: "--"
    property int readinessScore: 0
    property string actionFeedback: ""
    property var profilesModel: []

    function resolveEnginePath() {
        return Qt.resolvedUrl("../target/release/omaprot-engine").toString().replace(/^file:\/\//, "")
    }

    function refresh() {
        if (!probeProc.running) {
            probeProc.command = [resolveEnginePath(), "--", "--json"]
            probeProc.running = true
        }
    }

    function activateProfile(pid) {
        if (!cmdProc.running) {
            cmdProc.command = [resolveEnginePath(), "--", "--set-profile", pid]
            cmdProc.running = true
        }
    }

    function installSteamLayer() {
        if (!cmdProc.running) {
            cmdProc.command = [resolveEnginePath(), "--", "--install-steam-tool"]
            cmdProc.running = true
        }
    }

    function generatePkgbuild() {
        if (!cmdProc.running) {
            cmdProc.command = [resolveEnginePath(), "--", "--generate-pkgbuild"]
            cmdProc.running = true
        }
    }

    Process {
        id: probeProc
        running: false
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    var d = JSON.parse(this.text)
                    root.activeProfileId = d.active_profile_id || "bleeding-edge"
                    if (d.active_profile) {
                        root.activeProfileName = d.active_profile.name || ""
                        root.activeTagline = d.active_profile.tagline || ""
                    }
                    if (d.available_profiles) {
                        root.profilesModel = d.available_profiles
                    }
                    if (d.system) {
                        root.kernelRelease = d.system.kernel_release || ""
                        root.boreActive = Boolean(d.system.bore_scheduler_active)
                        root.adiosActive = Boolean(d.system.adios_io_scheduler)
                        root.ntsyncMod = Boolean(d.system.ntsync_module_present)
                        root.ntsyncDev = Boolean(d.system.ntsync_dev_active)
                        root.cpuModel = d.system.cpu_model || ""
                        root.isaLevel = d.system.isa_level || ""
                        root.marchTarget = d.system.march_target || ""
                        root.steamInstalled = Boolean(d.system.omaprot_installed)
                        root.steamInstallPath = d.system.omaprot_install_path || ""
                        root.readinessScore = d.system.readiness_score || 0
                    }
                    if (d.compiler_recipe) {
                        root.cflagsText = d.compiler_recipe.cflags || ""
                    }
                } catch (e) {}
            }
        }
    }

    Process {
        id: cmdProc
        running: false
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    var r = JSON.parse(this.text)
                    if (r.message) root.actionFeedback = r.message
                } catch (e) {}
                root.refresh()
            }
        }
    }

    Component.onCompleted: root.refresh()

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        // Header
        Rectangle {
            Layout.fillWidth: true
            height: 76
            radius: Theme.radiusMd
            color: Theme.bgSurface
            border.color: Theme.border
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.margins: 16
                spacing: 16

                Text {
                    text: Theme.iconRocket
                    font.family: root.fontFamily
                    font.pixelSize: 28
                    color: Theme.accentSuccess
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    Text {
                        text: "OMAPROTON 10.0 BLEEDING-EDGE • OMARCHY KERNEL & ISA ENGINE"
                        font.family: root.fontFamily
                        font.pixelSize: 16
                        font.bold: true
                        color: Theme.textMain
                    }

                    Text {
                        text: "Çekirdek: " + root.kernelRelease + " (BORE: " + (root.boreActive ? "Aktif" : "Pasif") + " · ADIOS: " + (root.adiosActive ? "Aktif" : "Pasif") + " · NTSYNC: " + (root.ntsyncMod ? "Mevcut" : "Yok") + ")"
                        font.family: root.fontFamily
                        font.pixelSize: 12
                        color: Theme.textMuted
                    }
                }

                Rectangle {
                    width: 120
                    height: 40
                    radius: Theme.radiusSm
                    color: Theme.bgCard
                    border.color: Theme.accentSuccess
                    border.width: 1

                    Text {
                        anchors.centerIn: parent
                        text: "SKOR: " + root.readinessScore + "/100"
                        font.family: root.fontFamily
                        font.pixelSize: 13
                        font.bold: true
                        color: Theme.accentSuccess
                    }
                }
            }
        }

        // Silicon & Compiler Strip
        Rectangle {
            Layout.fillWidth: true
            height: 92
            radius: Theme.radiusMd
            color: Theme.bgSurface
            border.color: Theme.border
            border.width: 1

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 14
                spacing: 6

                Text {
                    text: Theme.iconCpu + " İŞLEMCİ VE MİKROMİMARİ HEDEFİ: " + root.cpuModel + " -> [" + root.isaLevel + "]"
                    font.family: root.fontFamily
                    font.pixelSize: 13
                    font.bold: true
                    color: Theme.textMain
                }

                Text {
                    text: "CFLAGS: " + root.cflagsText
                    font.family: root.fontFamily
                    font.pixelSize: 11
                    color: Theme.textMuted
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Text {
                    text: "STEAM COMPATIBILITYTOOLS.D: " + (root.steamInstalled ? "KURULU (" + root.steamInstallPath + ")" : "KURULMADI")
                    font.family: root.fontFamily
                    font.pixelSize: 11
                    color: root.steamInstalled ? Theme.accentSuccess : Theme.accentWarning
                }
            }
        }

        // Profiles List
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Repeater {
                model: root.profilesModel

                Rectangle {
                    Layout.fillWidth: true
                    height: 110
                    radius: Theme.radiusMd
                    color: modelData.id === root.activeProfileId ? Theme.bgCardHover : Theme.bgCard
                    border.color: modelData.id === root.activeProfileId ? Theme.accentSuccess : Theme.border
                    border.width: modelData.id === root.activeProfileId ? 2 : 1

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 12
                        spacing: 6

                        Text {
                            text: modelData.name
                            font.family: root.fontFamily
                            font.pixelSize: 12
                            font.bold: true
                            color: Theme.textMain
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Text {
                            text: modelData.tagline
                            font.family: root.fontFamily
                            font.pixelSize: 10
                            color: Theme.textMuted
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: root.activateProfile(modelData.id)
                    }
                }
            }
        }

        // Action Buttons
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Rectangle {
                Layout.fillWidth: true
                height: 44
                radius: Theme.radiusSm
                color: Theme.bgCard
                border.color: Theme.accentSuccess
                border.width: 1

                Text {
                    anchors.centerIn: parent
                    text: Theme.iconCheck + " Steam Proton-Omarchy Katmanını Kur / Güncelle"
                    font.family: root.fontFamily
                    font.pixelSize: 12
                    font.bold: true
                    color: Theme.accentSuccess
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: root.installSteamLayer()
                }
            }

            Rectangle {
                Layout.fillWidth: true
                height: 44
                radius: Theme.radiusSm
                color: Theme.bgCard
                border.color: Theme.accent
                border.width: 1

                Text {
                    anchors.centerIn: parent
                    text: Theme.iconWrench + " Kaynak PKGBUILD (-march=" + root.marchTarget + ") Üret"
                    font.family: root.fontFamily
                    font.pixelSize: 12
                    font.bold: true
                    color: Theme.textMain
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: root.generatePkgbuild()
                }
            }
        }

        Text {
            visible: root.actionFeedback.length > 0
            text: root.actionFeedback
            font.family: root.fontFamily
            font.pixelSize: 12
            color: Theme.accentSuccess
        }
    }
}
