import SwiftUI

struct MainWindowView: View {
    @Bindable var store: CleanStore
    @Environment(\.openSettings) private var openSettings
    @State private var confirmClean = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
                .padding(.bottom, 20)
            categoryGrid
            Spacer(minLength: 16)
            statusArea
            Spacer(minLength: 16)
            footer
        }
        .padding(24)
        .frame(minWidth: 600, minHeight: 560)
        .navigationTitle("CleanWizard")
        .confirmationDialog("Move to Trash?", isPresented: $confirmClean, titleVisibility: .visible) {
            Button("Move \(Format.files(selectedFileCount)) to Trash", role: .destructive) {
                store.clean()
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private var selectedFileCount: Int {
        store.scanResult.items
            .filter { store.selection.contains($0.cleanType) }
            .reduce(0) { $0 + $1.fileCount }
    }

    private var headline: Int64 {
        store.phase == .finished ? store.lastTotal : store.selectedSize
    }

    private var header: some View {
        HStack(alignment: .center) {
            Text(Format.size(headline))
                .font(.system(size: 50, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .contentTransition(.numericText())
                .animation(.default, value: headline)
            Spacer()
            if store.isBusy {
                ProgressRing(value: store.progress, side: 44)
            }
        }
    }

    private var categoryGrid: some View {
        Grid(horizontalSpacing: 12, verticalSpacing: 12) {
            GridRow {
                card(.system)
                card(.app)
            }
            GridRow {
                card(.internet)
                card(.other)
            }
        }
    }

    private func card(_ category: CleanCategory) -> some View {
        CategoryCard(
            category: category,
            size: store.size(for: category),
            isSelected: store.isSelected(category),
            isEnabled: !store.isBusy
        ) {
            store.toggle(category)
        }
    }

    private var statusArea: some View {
        VStack(alignment: .leading, spacing: 8) {
            if store.isBusy {
                ProgressView(value: store.progress)
                    .progressViewStyle(.linear)
                    .animation(.easeInOut, value: store.progress)
                Text(store.currentPath)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            } else if let message = store.lastError {
                Label(message, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .lineLimit(2)
                    .truncationMode(.middle)
            } else if store.phase == .finished {
                Label("Moved \(Format.files(store.lastTrashed)) to Trash", systemImage: "checkmark.circle")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(.default, value: store.phase)
    }

    private var footer: some View {
        HStack(spacing: 12) {
            Button {
                openSettings()
            } label: {
                Image(systemName: "gearshape")
            }
            .buttonStyle(.borderless)
            .help("Settings")

            Spacer()

            Button(action: primaryAction) {
                Text(primaryTitle)
                    .frame(maxWidth: 220)
                    .padding(.vertical, 4)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .keyboardShortcut(.defaultAction)
            .disabled(!primaryEnabled)
        }
    }

    private var primaryTitle: String {
        switch store.phase {
        case .idle, .finished: "Scan"
        case .scanning: "Scanning"
        case .ready: store.scanResult.items.isEmpty ? "Nothing to Clean" : "Clean"
        case .cleaning: "Cleaning"
        }
    }

    private var primaryEnabled: Bool {
        switch store.phase {
        case .idle, .finished: true
        case .ready: !store.scanResult.items.isEmpty
        case .scanning, .cleaning: false
        }
    }

    private func primaryAction() {
        if store.phase == .ready {
            confirmClean = true
        } else {
            store.scan()
        }
    }
}
