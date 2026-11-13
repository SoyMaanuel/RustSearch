fn main() {
    #[cfg(windows)]
    {
        // Embebe app.rc (y por tanto icon.ico) en el .exe.
        // El segundo argumento es el manifiesto XML; None = no hay manifiesto.
        embed_resource::compile("app.rc", None::<&str>);
    }
}