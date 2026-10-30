import SwiftUI

struct MainWindowView: View {
    @Bindable var store: CleanStore
    @Environment(\.openSettings) private var openSettings
    @State private var confirmClean = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            categoryGrid
            itemList
            statusArea
            footer
        }
        .padding(20)
        .frame(minWidth: 640, idealWidth: 720, minHeight: 560, idealHeight: 680)
        .navigationTitle("CleanWizard")
        .confirmationDialog("Move to Trash?", isPresented: $confirmClean, titleVisibility: .visible) {
            Button("Move \(Format.files(store.selectedFileCount)) to Trash", role: .destructive) {
                store.clean()
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private var headline: Int64 {
        store.phase == .finished ? store.lastTotal : store.selectedSize
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(Format.size(headline))
                .font(.system(size: 44, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .contentTransition(.numericText())
                .animation(.default, value: headline)
            Spacer()
            if store.isBusy {
                ProgressRing(value: store.progress, side: 40)
            }
        }
    }

    private var categoryGrid: some View {
        LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
            ForEach(CleanCategory.scanOrder) { category in
                CategoryCard(
                    category: category,
                    size: store.size(for: category),
                    state: store.selectionState(for: category),
                    isEnabled: !store.isBusy && !store.items(for: category).isEmpty
                ) {
                    store.toggle(category)
                }
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }

    /// Sub-category rows, each with its own checkbox.
    private var itemList: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                ForEach(CleanCategory.scanOrder) { category in
                    let items = store.items(for: category)
                    if !items.isEmpty {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(category.title)
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 6)
                            ForEach(items) { item in
                                ItemRow(
                                    item: item,
                                    isSelected: store.isSelected(item),
                                    isEnabled: !store.isBusy
                                ) {
                                    store.toggle(item)
                                }
                            }
                        }
                    }
                }
            }
            .padding(.vertical, 4)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background.secondary, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            if store.scanResult.items.isEmpty && !store.isBusy {
                Text("No scan yet")
                    .font(.callout)
                    .foregroundStyle(.tertiary)
            }
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
        case .ready: "Clean"
        case .cleaning: "Cleaning"
        }
    }

    private var primaryEnabled: Bool {
        switch store.phase {
        case .idle, .finished: true
        case .ready: !store.selectedItems.isEmpty
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

/// One cleanable sub-category with its own checkbox.
struct ItemRow: View {
    let item: ScanItem
    let isSelected: Bool
    let isEnabled: Bool
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(isSelected ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                Text(item.title)
                    .lineLimit(1)
                    .foregroundStyle(.primary)
                if item.isCaution {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .help("Removes data you may still need")
                }
                Spacer(minLength: 8)
                Text(Format.size(item.size))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                Text(Format.files(item.fileCount))
                    .monospacedDigit()
                    .foregroundStyle(.tertiary)
                    .frame(width: 74, alignment: .trailing)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(isHovering ? AnyShapeStyle(.quaternary.opacity(0.35)) : AnyShapeStyle(.clear), in: .rect(cornerRadius: 6))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
        .onHover { isHovering = $0 }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(item.title)
        .accessibilityValue(Format.size(item.size))
        .accessibilityAddTraits(isSelected ? [.isSelected, .isButton] : .isButton)
    }
}
