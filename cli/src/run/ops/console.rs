use crate::errors::MycoError;
use crate::register_sync_op;
use crate::run::ops::macros::sync_op;
use crate::run::stack_trace::capture_call_site_stack;
use serde::Deserialize;
use v8;

#[derive(Deserialize)]
struct MessageArg {
    message: String,
}

#[derive(Deserialize)]
struct EmptyArg;

pub fn register_console_ops(
    scope: &mut v8::ContextScope<v8::HandleScope>,
    myco_ops: &v8::Object,
) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "print", sync_op_print);
    register_sync_op!(scope, myco_ops, "eprint", sync_op_eprint);
    register_sync_op!(scope, myco_ops, "trace", sync_op_trace);

    Ok(())
}

fn sync_op_print(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |_scope, input: MessageArg| -> Result<(), MycoError> {
            print!("{}", input.message);
            Ok(())
        },
    );
}

fn sync_op_eprint(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |_scope, input: MessageArg| -> Result<(), MycoError> {
            eprint!("{}", input.message);
            Ok(())
        },
    );
}

fn sync_op_trace(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, _input: EmptyArg| -> Result<String, MycoError> {
            let result = capture_call_site_stack(scope, 1);
            Ok(result)
        },
    );
}
