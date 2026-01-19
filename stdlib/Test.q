// Q语言标准测试库
// package: std.Test
//
// 提供基本的测试断言功能

package std.Test

/// 断言条件为真
public func assert(condition: bool, message: string = "Assertion failed") {
    if !condition {
        panic(message)
    }
}

/// 断言两个值相等
public func assertEqual(actual: int, expected: int, message: string = "") {
    if actual != expected {
        var msg = message
        if msg == "" {
            msg = "Expected ${expected}, but got ${actual}"
        }
        panic(msg)
    }
}

/// 断言两个字符串相等
public func assertEqualStr(actual: string, expected: string, message: string = "") {
    if actual != expected {
        var msg = message
        if msg == "" {
            msg = "Expected '${expected}', but got '${actual}'"
        }
        panic(msg)
    }
}

/// 断言条件为真
public func assertTrue(condition: bool, message: string = "Expected true, but got false") {
    if !condition {
        panic(message)
    }
}

/// 断言条件为假
public func assertFalse(condition: bool, message: string = "Expected false, but got true") {
    if condition {
        panic(message)
    }
}

/// 测试失败
public func fail(message: string = "Test failed") {
    panic(message)
}

/// 打印测试通过信息
public func pass(testName: string) {
    println("[PASS] ${testName}")
}

/// 打印测试开始信息
public func begin(testName: string) {
    println("[TEST] ${testName}")
}
