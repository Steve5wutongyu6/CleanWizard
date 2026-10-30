import SwiftUI

struct CategoryCard: View {
    let category: CleanCategory
    let size: Int64
    let state: SelectionState
    let isEnabled: Bool
    let action: () -> Void

    @State private var isHovering = false

    private var isSelected: Bool { state != .none }
    private var isActive: Bool { state == .all }

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(systemName: category.symbol)
                    .font(.system(size: 22, weight: .regular))
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)

                VStack(spacing: 2) {
                    Text(category.title)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.primary)
                    Text(Format.size(size))
                        .font(.callout.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .background(background)
            .overlay(alignment: .topTrailing) {
                if state == .partial {
                    Image(systemName: "minus.circle.fill")
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, Color.accentColor)
                        .padding(8)
                }
            }
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
        .onHover { isHovering = $0 }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(category.title)
        .accessibilityValue(Format.size(size))
        .accessibilityAddTraits(isActive ? [.isSelected, .isButton] : .isButton)
    }

    private var background: some View {
        RoundedRectangle(cornerRadius: 14, style: .continuous)
            .fill(isSelected ? Color.accentColor.opacity(0.12) : Color(nsColor: .controlBackgroundColor))
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .strokeBorder(isSelected ? Color.accentColor : .clear, lineWidth: 1)
            }
            .opacity(isHovering && !isActive ? 0.92 : 1)
    }
}
