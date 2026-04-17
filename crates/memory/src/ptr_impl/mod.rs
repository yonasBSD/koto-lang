cfg_select! {
    feature = "arc" => {
        mod arc;
        pub(crate) use arc::*;
    }
    feature = "rc" => {
        mod rc;
        pub(crate) use rc::*;
    }
}
