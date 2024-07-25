# tauri-bindgen-rs-macros

![rust workflow](https://github.com/jvatic/tauri-bindgen-rs-macros/actions/workflows/rust.yml/badge.svg)

This is an *experimental* crate to aid in generating Rust IPC bindings for [Tauri](https://tauri.app/) commands (for those of us who'd like to use Rust for the front-end). Please review the code before using it.

## Why

I couldn't find a comfortable way of defining commands that would maintain type safety with Tauri IPC bindings for a Rust frontend. So this is a crude attempt at solving this without changing too much about how the commands are defined.

## Usage

1. Create an intermediary crate in the workspace of your Tauri app to house traits defining your commands and generated IPC bindings to import into the Rust frontend, e.g:

    ```toml
    [package]
    edition = "2021"
    name = "my-commands"
    version = "0.1.0"

    [dependencies]
    tauri-bindgen-rs-macros = { version = "0.1.0", git = "https://github.com/jvatic/tauri-bindgen-rs-macros.git" }
    serde = { version = "1.0.204", features = ["derive"] }
    serde-wasm-bindgen = "0.6"
    wasm-bindgen = "0.2"
    wasm-bindgen-futures = "0.4"
    ```

    ```rust
    #[allow(async_fn_in_trait)]
    #[tauri_bindgen_rs_macros::invoke_bindings]
    pub trait Commands {
        async hello(name: String) -> Result<String, String>;
    }
    ```

    And if you're using a plugin on the frontend and want bindings generated for it, you can do so by defining a trait for it, e.g:

    ```rust
    pub mod someplugin {
        #[allow(async_fn_in_trait)]
        #[tauri_bindgen_rs_macros::invoke_bindings(cmd_prefix = "plugin:some-plugin|")]
        pub trait SomePlugin {
            // ...
        }
    }
    ```

    **NOTE:** If you have multiple traits implementing `invoke_bindings` they'll each need to be in their own `mod` since an `invoke` WASM binding will be derived in scope of where the trait is defined.

2. Import the trait into your Tauri backend and wrap your command definitions in the `impl_trait` macro, e.g:

    ```rust
    use my_commands::Commands;
    tauri_bindgen_rs_macros::impl_trait!(Commands, {
        #[tauri::command]
        async hello(state: tauri::State, name: String) -> Result<String, String> {
            Ok(format!("Hello {}", name))
        }
    });
    ```

    This will define a shadow struct with an `impl Commands` block with all the functions passed into the macro minus any fn generics or arguments where the type starts with `tauri::`, and spits out the actual fns untouched. The Rust compiler will then emit helpful errors if the defined commands are different (after being processed) from those in the trait, yay!

3. Use the generated IPC bindings in your Rust frontend, eg:

    ```rust
    // ...
    spawn_local(async move {
        let greeting = my_commands::hello(name).await.unwrap();
        set_greeting(greeting);
    });
    // ...
    ```
