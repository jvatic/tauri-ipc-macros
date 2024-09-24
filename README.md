# tauri-ipc-macros

![rust workflow](https://github.com/jvatic/tauri-ipc-macros/actions/workflows/rust.yml/badge.svg)

IPC bindings for using [Tauri](https://v2.tauri.app/) with a Rust Frontend (e.g.
[leptos](https://v2.tauri.app/start/frontend/leptos/)).

**NOTE:** The API is currently unstable and may change.

## Why

I couldn't find a comfortable way of defining commands that would maintain type
safety with Tauri IPC bindings for a Rust Frontend. So this is a crude attempt
at solving this without changing too much about how the commands are defined.

## Usage

1. Create an intermediary crate in the workspace of your Tauri app to house traits defining your commands, events, and generated IPC bindings to import into the Rust frontend, e.g:

    ```toml
    [package]
    edition = "2021"
    name = "my-commands"
    version = "0.1.0"

    [dependencies]
    tauri-ipc-macros = { version = "0.1.2", git = "https://github.com/jvatic/tauri-ipc-macros.git" }
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

    #[derive(tauri_bindgen_rs_macros::Events, Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
    enum Event {
        SomethingHappened { payload: Vec<u8> },
        SomeoneSaidHello(String),
        NoPayload,
    }
    ```

    **NOTE:** If you have multiple enums deriving `Events`, these will need to
    be in separate modules since there's some common boilerplate types that are
    included currently (that will be moved into another crate at some point).

    And if you're using a plugin on the frontend and want bindings generated for
    it, you can do so by defining a trait for it, e.g:

    ```rust
    pub mod someplugin {
        #[allow(async_fn_in_trait)]
        #[tauri_bindgen_rs_macros::invoke_bindings(cmd_prefix = "plugin:some-plugin|")]
        pub trait SomePlugin {
            // ...
        }
    }
    ```

    **NOTE:** You can find the `cmd_prefix` and plugin API by looking at the
    [guest-js](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/clipboard-manager/guest-js/index.ts)
    bindings and [Rust
    source](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/clipboard-manager/src/commands.rs)
    for the plugin(s) you're using.

    **NOTE:** If you have multiple traits implementing `invoke_bindings` they'll
    each need to be in their own `mod` since an `invoke` WASM binding will be
    derived in scope of where the trait is defined (this will be moved into
    another module at some point).

2. Import the commands trait into your Tauri backend and wrap your command definitions in the `impl_trait` macro, e.g:

    ```rust
    use my_commands::Commands;
    tauri_bindgen_rs_macros::impl_trait!(Commands, {
        #[tauri::command]
        async hello(state: tauri::State, name: String) -> Result<String, String> {
            Ok(format!("Hello {}", name))
        }
    });
    ```

    This will define a new struct named `__ImplCommands` with an `impl Commands
    for __ImplCommands` block with all the fns passed into the macro (minus any
    fn generics or arguments where the type starts with `tauri::`), and spits
    out the actual fns untouched. The Rust compiler will then emit helpful
    errors if the defined commands are different (after being processed) from
    those in the trait, yay!

    **NOTE:** The crudeness here is due to `#[tauri::command]`s needing to be
    top level fns and potentially having additional arguments in the siganture.
    And while I can imagine a way of abstracting this out of the API (so this
    could be a regular `impl` block), this was the easiest thing and works
    without changing much about how the commands are defined.

3. Import the event enum into your Tauri backend if you wish to emit events from there, e.g.:

    ```rust
    use my_commands::Event;
    fn emit_event(app_handle: tauri::AppHandle, event: Event) -> anyhow::Result<()> {
        Ok(app_handle.emit(event.event_name(), event)?)
    }
    ```

3. Use the generated IPC bindings in your Rust frontend, eg:

    ```rust
    // ...
    spawn_local(async move {
        let greeting = my_commands::hello(name).await.unwrap();
        set_greeting(greeting);
    });
    // ...
    spawn_local(async move {
        let listener = my_commands::EventBinding::SomethingHappened.listen(|event: my_commands::Event| {
            // ...
        }).await;
        drop(listener); // unlisten
    });
    // ...
    ```
