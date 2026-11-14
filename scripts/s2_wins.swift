// Lists on-screen windows of the named apps: owner, window id, layer, width, height.
import CoreGraphics
import Foundation
let want = CommandLine.arguments.dropFirst().map { $0 }
let l = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in l {
    let o = w[kCGWindowOwnerName as String] as? String ?? ""
    if want.contains(o) {
        let b = w[kCGWindowBounds as String] as! [String: Any]
        print(o, w[kCGWindowNumber as String]!, w[kCGWindowLayer as String]!, b["Width"]!, b["Height"]!)
    }
}
