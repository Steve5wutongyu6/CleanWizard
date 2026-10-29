import SwiftUI

struct SettingsView: View {
    @Bindable var store: CleanStore
    @State private var report: String?
    @State private var failed = false

    var body: some View {
        Form {
            Section("Rules") {
                Picker("Source", selection: $store.source) {
                    ForEach(RuleSource.allCases) { source in
                        Label(source.title, systemImage: source.symbol).tag(source)
                    }
                }
                .pickerStyle(.radioGroup)

                LabeledContent("Version") {
                    Text(store.rulesVersion.isEmpty ? "—" : store.rulesVersion)
                        .textSelection(.enabled)
                }

                HStack {
                    Button("Export from Lemon", action: exportFromLemon)
                        .disabled(!Bridge.isLemonInstalled)
                    Button("Open Rules Folder", action: openRulesFolder)
                }
            }

            if let report {
                Section("Last Export") {
                    Text(report)
                        .foregroundStyle(failed ? .orange : .secondary)
                        .textSelection(.enabled)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .fixedSize(horizontal: false, vertical: true)
    }

    private func exportFromLemon() {
        let destination = RuleSource.lemon.rulesDirectory(root: store.root)
        let result = Bridge.dumpLemonRules(to: destination)
        switch result.status {
        case .success:
            failed = result.ruleFileCount == 0
            report = result.ruleFileCount == 0
                ? "Knowledge folder is empty · \(result.written.count) files written"
                : "\(result.ruleFileCount) rule files · \(result.written.count) files written"
            if result.ruleFileCount > 0 {
                store.source = .lemon
            }
            store.loadRules()
        case .lemonNotFound:
            failed = true
            report = "Tencent Lemon not installed"
        case .noKnowledgeRules:
            failed = true
            report = "Lemon knowledge folder empty"
        }
    }

    private func openRulesFolder() {
        NSWorkspace.shared.open(store.rulesDirectory)
    }
}
