// Q语言标准异常库 - IO异常
// package: std.lang

package std.lang

import std.lang.{Exception, Throwable}

/// IO 异常 - 输入输出错误
class IOException extends Exception {
    func toString() string {
        return "IOException: " + this.message
    }
}

/// 文件未找到异常
class FileNotFoundException extends IOException {
    /// 文件路径
    var path: string
    
    func toString() string {
        return "FileNotFoundException: " + this.path
    }
}

/// 文件已存在异常
class FileAlreadyExistsException extends IOException {
    /// 文件路径
    var path: string
    
    func toString() string {
        return "FileAlreadyExistsException: " + this.path
    }
}

/// 权限拒绝异常
class PermissionDeniedException extends IOException {
    /// 资源路径
    var path: string
    
    func toString() string {
        return "PermissionDeniedException: " + this.path
    }
}

/// 文件结束异常
class EOFException extends IOException {
    func toString() string {
        return "EOFException: " + this.message
    }
}

/// 网络异常
class NetworkException extends IOException {
    func toString() string {
        return "NetworkException: " + this.message
    }
}

/// 连接超时异常
class TimeoutException extends IOException {
    func toString() string {
        return "TimeoutException: " + this.message
    }
}
