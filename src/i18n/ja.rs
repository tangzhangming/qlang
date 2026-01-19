//! 日本語メッセージ

use super::messages::*;

/// 日本語メッセージを取得
pub fn get(key: &str) -> &'static str {
    match key {
        // コンパイルエラー
        ERR_COMPILE_UNEXPECTED_TOKEN => "予期しないトークン: {}",
        ERR_COMPILE_EXPECTED_EXPRESSION => "式が期待されます",
        ERR_COMPILE_UNTERMINATED_STRING => "文字列が閉じられていません",
        ERR_COMPILE_INVALID_NUMBER => "無効な数値: {}",
        ERR_COMPILE_EXPECTED_TOKEN => "'{}' が期待されましたが、'{}' が見つかりました",
        ERR_COMPILE_EXPECTED_TYPE => "型注釈が期待されます",
        ERR_COMPILE_EXPECTED_IDENTIFIER => "識別子が期待されます",
        ERR_COMPILE_UNDEFINED_VARIABLE => "未定義の変数: '{}'",
        ERR_COMPILE_VARIABLE_ALREADY_DEFINED => "変数 '{}' はこのスコープで既に定義されています",
        ERR_COMPILE_CANNOT_ASSIGN_TO_CONST => "定数 '{}' に代入できません",
        ERR_COMPILE_TYPE_MISMATCH => "型の不一致: '{}' が期待されましたが、'{}' が見つかりました",
        ERR_COMPILE_BREAK_OUTSIDE_LOOP => "'break' はループ内でのみ使用できます",
        ERR_COMPILE_CONTINUE_OUTSIDE_LOOP => "'continue' はループ内でのみ使用できます",
        ERR_COMPILE_UNKNOWN_FUNCTION => "未知の関数: '{}'",
        ERR_COMPILE_CATCH_MISSING_TYPE => "catch パラメータには型注釈が必要です。例: catch (e:Exception)",
        
        // 型チェックエラー
        ERR_TYPE_UNDEFINED_TYPE => "未定義の型: '{}'",
        ERR_TYPE_INCOMPATIBLE => "互換性のない型: '{}' と '{}'",
        ERR_TYPE_CANNOT_CALL => "関数でない型 '{}' を呼び出せません",
        ERR_TYPE_WRONG_ARG_COUNT => "{} 個の引数が期待されましたが、{} 個が見つかりました",
        ERR_TYPE_UNDEFINED_FIELD => "型 '{}' にフィールド '{}' がありません",
        ERR_TYPE_UNDEFINED_METHOD => "型 '{}' にメソッド '{}' がありません",
        ERR_TYPE_CANNOT_INDEX => "型 '{}' をインデックスアクセスできません",
        ERR_TYPE_CANNOT_ITERATE => "型 '{}' を反復できません",
        ERR_TYPE_NOT_NULLABLE => "型 '{}' は null 許容ではありません。'?' を使用して null 許容にしてください",
        ERR_TYPE_ABSTRACT_INSTANTIATE => "抽象クラス '{}' をインスタンス化できません",
        ERR_TYPE_TRAIT_NOT_IMPL => "型 '{}' はトレイト '{}' を実装していません",
        ERR_TYPE_GENERIC_ARGS => "型引数の数が違います: {} 個が期待されましたが、{} 個が見つかりました",
        
        // 実行時エラー
        ERR_RUNTIME_DIVISION_BY_ZERO => "ゼロ除算エラー",
        ERR_RUNTIME_TYPE_MISMATCH => "実行時型エラー: '{}' が期待されましたが、'{}' が見つかりました",
        ERR_RUNTIME_STACK_OVERFLOW => "スタックオーバーフロー",
        ERR_RUNTIME_STACK_UNDERFLOW => "スタックアンダーフロー",
        ERR_RUNTIME_INDEX_OUT_OF_BOUNDS => "インデックス {} は範囲外です（長さ: {}）",
        ERR_RUNTIME_NULL_POINTER => "ヌルポインタ参照",
        ERR_RUNTIME_ASSERTION_FAILED => "アサーション失敗: {}",
        ERR_RUNTIME_INVALID_OPERATION => "無効な操作: {}",
        
        // 並行処理エラー
        ERR_CONCURRENT_CHANNEL_CLOSED => "閉じられたチャネルに送信できません",
        ERR_CONCURRENT_DEADLOCK => "デッドロックの可能性を検出しました",
        ERR_CONCURRENT_SEND_FAILED => "チャネルへの送信に失敗しました",
        ERR_CONCURRENT_RECV_FAILED => "チャネルからの受信に失敗しました",
        ERR_CONCURRENT_MUTEX_POISONED => "ミューテックスが汚染されています（ロックを保持しているスレッドがパニックしました）",
        
        // GCメッセージ
        MSG_GC_STARTED => "GC開始（世代: {}）",
        MSG_GC_COMPLETED => "GC完了、所要時間 {} ミリ秒",
        MSG_GC_FREED => "GCが {} 個のオブジェクト（{} バイト）を解放しました",
        
        // CLIメッセージ
        MSG_CLI_USAGE => "使用法: {} <コマンド> [オプション] <ファイル>",
        MSG_CLI_VERSION => "{} バージョン {}",
        MSG_CLI_COMPILING => "{} をコンパイル中...",
        MSG_CLI_RUNNING => "{} を実行中...",
        MSG_CLI_DONE => "完了。",
        MSG_CLI_ERROR => "エラー: {}",
        MSG_CLI_FILE_NOT_FOUND => "ファイルが見つかりません: {}",
        MSG_CLI_INVALID_EXTENSION => "無効なファイル拡張子: '{}'。'.{}' ファイルを使用してください",
        MSG_CLI_HELP => "Q言語 - モダンで本番環境対応のプログラミング言語",
        MSG_CLI_COMMANDS => "コマンド:\n  run <ファイル>     Qソースファイルを実行\n  build <ファイル>   Qソースファイルをコンパイル\n  repl           インタラクティブREPLを開始\n  help           このヘルプメッセージを表示",
        
        // ヒント
        HINT_DID_YOU_MEAN => "'{}' のことですか？",
        HINT_CHECK_SPELLING => "スペルを確認するか、その項目が定義されていることを確認してください",
        HINT_MISSING_IMPORT => "このモジュールを先にインポートする必要があるかもしれません",
        HINT_TYPE_ANNOTATION => "ここに型注釈を追加することを検討してください",
        HINT_USE_NULL_CHECK => "安全なアクセスには '?.' を、非ヌルアサーションには '!' を使用してください",
        
        // 未知のメッセージキー
        _ => "未知のメッセージキー",
    }
}
