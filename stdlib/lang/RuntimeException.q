// Q语言标准异常库 - 运行时异常
// package: std.lang

package std.lang

import std.lang.{Exception, Throwable}

/// 运行时异常 - 程序运行过程中发生的异常
class RuntimeException extends Exception {
    func toString() string {
        if this.location != "" {
            return "RuntimeException: " + this.message + " at " + this.location
        }
        return "RuntimeException: " + this.message
    }
}

/// 空指针异常 - 尝试对 null 值进行操作
class NullPointerException extends RuntimeException {
    func toString() string {
        return "NullPointerException: " + this.message
    }
}

/// 数组越界异常 - 数组索引超出范围
class IndexOutOfBoundsException extends RuntimeException {
    /// 实际索引值
    var index: int
    /// 数组长度
    var length: int
    
    func toString() string {
        return "IndexOutOfBoundsException: index out of bounds"
    }
}

/// 非法参数异常 - 传入的参数不合法
class IllegalArgumentException extends RuntimeException {
    func toString() string {
        return "IllegalArgumentException: " + this.message
    }
}

/// 算术异常 - 算术运算错误（如除零）
class ArithmeticException extends RuntimeException {
    func toString() string {
        return "ArithmeticException: " + this.message
    }
}

/// 类型转换异常 - 类型转换失败
class ClassCastException extends RuntimeException {
    /// 源类型
    var sourceType: string
    /// 目标类型
    var targetType: string
    
    func toString() string {
        return "ClassCastException: cannot cast type"
    }
}

/// 不支持的操作异常
class UnsupportedOperationException extends RuntimeException {
    func toString() string {
        return "UnsupportedOperationException: " + this.message
    }
}

/// 非法状态异常 - 对象处于不正确的状态
class IllegalStateException extends RuntimeException {
    func toString() string {
        return "IllegalStateException: " + this.message
    }
}

/// 数字格式异常 - 字符串无法转换为数字
class NumberFormatException extends RuntimeException {
    func toString() string {
        return "NumberFormatException: " + this.message
    }
}
