# BOS Component Loader

Serves a local directory of component files as a JSON payload properly formatted to be plugged into a BOS `redirectMap`. When paired with a viewer configured to call out to this loader, it enables local component development—especially when working on multiple components in parallel.

Works best when paired with [FroVolod/bos-cli-rs](https://github.com/FroVolod/bos-cli-rs) for component syncing and CI/CD

## Installation

see GitHub Releases

## Usage

1. Run this tool with desired options

```sh
Serves the contents of BOS component files (.jsx) in a specified directory as a JSON object properly formatted for preview on a BOS gateway

Usage: bos-loader [OPTIONS] <ACCOUNT_ID>

Arguments:
  <ACCOUNT_ID>
          NEAR account to use as component author in preview

Options:
  -p, --path <PATH>
          Path to directory containing component files

          [default: .]

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
