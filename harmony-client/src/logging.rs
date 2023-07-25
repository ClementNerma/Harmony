use std::sync::atomic::AtomicBool;

pub static PRINT_DEBUG_MESSAGES: AtomicBool = AtomicBool::new(false);
pub static PRINT_MESSAGES_DATETIME: AtomicBool = AtomicBool::new(false);

#[macro_export]
macro_rules! _format {
    ($color: ident => $message: tt, $($params: tt)*) => {{
        use colored::Colorize;
        let mut msg = format!($message, $($params)*);

        if $crate::logging::PRINT_MESSAGES_DATETIME.load(::std::sync::atomic::Ordering::Relaxed) {
            msg = format!("[{:?}] {msg}", ::std::time::SystemTime::now());
        }

        msg.$color()
    }}
}

#[macro_export]
macro_rules! error {
    ($message: tt, $($params: tt)*) => {{
        eprintln!("{}", $crate::_format!(bright_red => $message, $($params)*));
    }};

    ($message: tt) => {{
        error!($message,)
    }};
}

#[macro_export]
macro_rules! error_anyhow {
    ($error: expr) => {{
        use colored::Colorize;
        eprintln!("{}", format!("{:?}", $error).bright_red());
    }};
}

#[macro_export]
macro_rules! warn {
    ($message: tt, $($params: tt)*) => {{
        eprintln!("{}", $crate::_format!(bright_yellow => $message, $($params)*));
    }};

    ($message: tt) => {{
        warn!($message,)
    }};
}

#[macro_export]
macro_rules! info {
    ($message: tt, $($params: tt)*) => {{
        println!("{}", $crate::_format!(bright_blue => $message, $($params)*));
    }};

    ($message: tt) => {{
        info!($message,)
    }};
}

#[macro_export]
macro_rules! info_inline {
    ($message: tt, $($params: tt)*) => {{
        print!("{}", $crate::_format!(bright_blue => $message, $($params)*));
    }};

    ($message: tt) => {{
        info_inline!($message,)
    }};
}

#[macro_export]
macro_rules! notice {
    ($message: tt, $($params: tt)*) => {{
        println!("{}", $crate::_format!(bright_cyan => $message, $($params)*));
    }};

    ($message: tt) => {{
        notice!($message,)
    }};
}

#[macro_export]
macro_rules! debug {
    ($message: tt, $($params: tt)*) => {{
        if $crate::logging::PRINT_DEBUG_MESSAGES.load(::std::sync::atomic::Ordering::SeqCst) {
            println!("{}", $crate::_format!(bright_cyan => $message, $($params)*));
        }
    }};

    ($message: tt) => {{
        debug!($message,)
    }};
}

#[macro_export]
macro_rules! success {
    ($message: tt, $($params: tt)*) => {{
        println!("{}", $crate::_format!(bright_green => $message, $($params)*));
    }};

    ($message: tt) => {{
        success!($message,)
    }};
}
