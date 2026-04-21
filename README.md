# envsync

A simple tool to sync environment variables between different machines. It uses a simple JSON file to store the environment variables and their values. The tool can be used to sync environment variables between different machines, or to backup and restore environment variables.

## Installation

To install envsync, you can use cargo:

```bash
cargo install envsync
```

## Usage

To use envsync, you can run the following command:

```bash
envsync -h
```

This will show you the help message with all the available options. The most common usage is to sync environment variables from a source machine to a target machine. You can do this by running the following command on the source machine:

```bash
envsync serve
```

This will start a server that makes the database persistent (otherwise it will be stored in memory and lost when the server is stopped). Then, on the target machine, you can run the following command:

```bash
envsync execute --profile my_company --remote-id <output of envsync serve>
```

This will open the bash terminal with the environment variables from the source machine on the target machine. You can also specify a profile to use, which allows you to have different sets of environment variables for different purposes.

## Contributing

If you want to contribute to envsync, you can fork the repository and create a pull request with your changes. You can also open an issue if you have any suggestions or find any bugs.

## License

envsync is licensed under the MIT License. See the LICENSE file for more details.
