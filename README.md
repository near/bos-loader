# BOS Component Loader

Serves a local directory of component files as a JSON payload properly formatted to be plugged into a BOS `redirectMap`. When paired with a viewer configured to call out to this loader, it enables local component development—especially when working on multiple components in parallel.

Works best when paired with [FroVolod/bos-cli-rs](https://github.com/FroVolod/bos-cli-rs) for component syncing and CI/CD

## Installation

see GitHub Releases

## Compatibility
Should work without issue when accessing gateway through Chrome, Arc and Firefox.

Brave requires turning shields off for gateway site.

Safari requires serving over HTTPS, which can be accomplished with ngrok. See [this issue](https://github.com/near/bos-loader/issues/9)

## Usage

1. Run this tool with desired options

```sh
Serves the contents of BOS component files (.jsx) in a specified directory as a JSON object properly formatted for preview on a BOS gateway

Usage: bos-loader [OPTIONS] [ACCOUNT_ID]

Arguments:
  [ACCOUNT_ID]
          NEAR account to use as component author in preview

Options:
  -p, --path <PATH>
          Path to directory containing component files
          
          [default: .]

  -c
          Use config file in current dir (./.bos-loader.toml) to set account_id and path, causes other args to be ignored

  -w
          Run in BOS Web Engine mode

      --port <PORT>
          Port to serve on
          
          [default: 3030]

  -r, --replacements <REPLACEMENTS>
          Path to file with replacements map

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

The only required argument is the account which you want to serve the components from

e.g. running from a directory with `HelloWorld.jsx` in the following way

```bash
bos-loader michaelpeter.near
```

results in

```json
{
  "components": {
    "michaelpeter.near/widget/HelloWorld": {
      "code": "return <>Hello World</>;"
    }
  }
}
```

2. Go to https://near.org/flags and set the BOS Loader URL to access your bos-loader instance. The default would be `http://127.0.0.1:3030`
3. Load the component you would like to preview as `https://near.org/<account id>/widget/<component name>`
   - e.g. from the previous example: `https://near.org/michaelpeter.near/widget/HelloWorld`

## Replacements

The replacements file is an optional file where placeholders and values they should resolve to are specified. Think of replacements as environment variables for your components which are injected before writing the component code on chain

The file should have the following format:

```json
{
  "REPL_PLACEHOLDER1": "value1",
  "REPL_PLACEHOLDER2": "value2"
}
```

The placeholders in widgets are replaced with specified values. For example the code for the following widget:

```javascript
return <>
    <div> This is ${REPL_PLACEHOLDER1} </div>
    <Widget src="${REPL_ACCOUNT}/widget/SomeWidget">
    <div>${REPL_PLACEHOLDER2}</div>
</>;
```

will be resolved to:

```javascript
return <>
    <div> This is value1 </div>
    <Widget src="accountId/widget/SomeWidget">
    <div>value2</div>
</>;
```

where accountId is the account passed as an argument.

The file should **not** contain `REPL_ACCOUNT` placeholder. This placeholder is automatically resolved to `accountId` value.

## Configuration file

Some advanced options can be configured via a `.bos-loader.toml` file in the directory where you run the loader. The following options are available

### paths

specify multiple accounts and paths to serve components from. You can even serve components from the same directory as multiple accounts

```toml
paths = [
  { account = "near", path = "./components" },
  { account = "michaelpeter.near", path = "./src" },
]
```

## Multi-device Testing

Run both your loader behind [ngrok](https://ngrok.com/) to test on multiple devices or share your working copy with others!

Example ngrok config:

```yml
authtoken: <automatically populated during setup>
tunnels:
  api:
    proto: http
    addr: 127.0.0.1:3030
    subdomain: my-loader # change this and use as your loader url e.g. https://my-loader.ngrok.io
version: "2"
region: us
```

Then start with `ngrok start --all`

## Contributing

### Cutting a new release

Once all changes are merged into `main`, use `cargo release` to cut a new release. This will automatically update the version in `Cargo.toml`, create a new git tag, and push the tag to GitHub.

Given the next release version will be `0.9.0`

```bash
# dry run to make sure everything looks normal
cargo release 0.9.0

# execute the release
cargo release 0.9.0 --execute
```