fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("asset/logo.ico");
        res.compile().unwrap();
    }
}
