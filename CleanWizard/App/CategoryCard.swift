import SwiftUI

struct CategoryCard: View {
    let category: CleanCategory
    let size: Int64
    let isSelected: Bool
    let isEnabled: Bool
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            VStack(spacing: 10) {
                Image(systemName: category.symbol)
                    .font(.system(size: 26, weight: .regular))
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)

                VStack(spacing: 2) {
                    Text(category.title)
                        .font(.headline)
                        .foregroundStyle(.primary)
                    Text(Format.size(size))
                        .font(.title3.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 18)
            .background(background)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
        .onHover { isHovering = $0 }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(category.title)
        .accessibilityValue(Format.size(size))
        .accessibilityAddTraits(isSelected ? [.isSelected, .isButton] : .isButton)
    }

    private var background: some View {
        RoundedRectangle(cornerRadius: 14, style: .continuous)
            .fill(isSelected ? Color.accentColor.opacity(0.12) : Color(nsColor: .controlBackgroundColor))
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .strokeBorder(isSelected ? Color.accentColor : .clear, lineWidth: 1)
            }
            .opacity(isHovering && !isSelected ? 0.92 : 1)
    }
}
