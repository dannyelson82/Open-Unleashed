// The engine's text prompts (player name, server address, chat) as tvOS's own full-screen keyboard.
// Replaces a UIAlertController with a text field: on tvOS that opened the keyboard, and the keyboard's Done
// only returned to the alert, which still needed OK, so it looked as if it asked again (real Apple TV,
// 2026-11-26). Here Done enters the text and Menu cancels. GPL-3.0-or-later.
import UIKit

final class TextEntryViewController: UIViewController, UITextFieldDelegate {
    private let field = UITextField()
    private let titleText: String
    private let current: String
    private let maxLen: Int
    private let secure: Bool
    private let finish: (String?) -> Void
    private var finished = false
    var isFinished: Bool { finished }
    /// The shell gave up on showing it (and answered for it).
    func markFinished() { finished = true }

    init(title: String, current: String, maxLen: Int, secure: Bool, finish: @escaping (String?) -> Void) {
        titleText = title
        self.current = current
        self.maxLen = maxLen
        self.secure = secure
        self.finish = finish
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .overFullScreen
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor.black.withAlphaComponent(0.85)
        field.text = current
        field.placeholder = titleText
        field.isSecureTextEntry = secure
        field.autocapitalizationType = .none
        field.autocorrectionType = .no
        field.returnKeyType = .done
        field.delegate = self
        field.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(field)
        NSLayoutConstraint.activate([
            field.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            field.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            field.widthAnchor.constraint(equalToConstant: 900),
        ])
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        field.becomeFirstResponder()   // straight to the full-screen keyboard
    }

    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
        guard maxLen > 0, let t = textField.text, let r = Range(range, in: t) else { return true }
        return t.replacingCharacters(in: r, with: string).count <= maxLen
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        done(textField.text ?? "")
        return true
    }

    // Done ends editing as "committed"; Menu as "cancelled"
    func textFieldDidEndEditing(_ textField: UITextField, reason: UITextField.DidEndEditingReason) {
        done(reason == .committed ? (textField.text ?? "") : nil)
    }

    private func done(_ text: String?) {
        guard !finished else { return }
        finished = true
        let clipped = text.map { String($0.prefix(maxLen > 0 ? maxLen : 256)) }
        dismiss(animated: true) { [finish] in finish(clipped) }
    }
}
