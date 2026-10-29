import SwiftUI

struct AboutView: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "sparkles")
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(.tint)
            Text("CleanWizard")
                .font(.title2.weight(.semibold))
            Text(Engine.version.map { "Engine \($0)" } ?? "Engine unavailable")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .padding(28)
        .frame(width: 300)
    }
}
