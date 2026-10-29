import SwiftUI

struct ProgressRing: View {
    let value: Double
    let side: CGFloat

    var body: some View {
        Gauge(value: clamped) {
            EmptyView()
        } currentValueLabel: {
            Text(percent)
                .font(.system(size: side * 0.24, weight: .semibold))
                .monospacedDigit()
        }
        .gaugeStyle(.accessoryCircular)
        .tint(.accentColor)
        .frame(width: side, height: side)
        .animation(.easeInOut, value: clamped)
    }

    private var clamped: Double { min(max(value, 0), 1) }

    private var percent: String { "\(Int((clamped * 100).rounded()))%" }
}
