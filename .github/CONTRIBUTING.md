## Reporting Issues

If you encounter any bugs or have feature requests, please open an issue on our issue tracker. When reporting an issue, please include:
- A clear and descriptive title.
- A detailed description of the problem or feature request.
- Steps to reproduce the issue (if applicable).
- Any relevant screenshots or logs.

## Contributing

We welcome contributions to Chromascope! If you have suggestions for new features, bug reports, or would like to contribute code, please open an issue or submit a pull request on our [GitHub repository](https://github.com/chromascope-ms/chromascope).

### Development Setup

1. **Fork the Repository**:
   - Create a fork of the Chromascope repository on GitHub.

2. **Clone Your Fork**:
   ```bash
   git clone https://github.com/chromascope-ms/chromascope.git
   cd chromascope
   ```

3. **Create a New Branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

4. **Make Your Changes**:
   - Implement your changes, then commit them.

5. **Push to Your Fork**:
   ```bash
   git push origin feature/your-feature-name
   ```

6. **Submit a Pull Request**:
   - Open a pull request on the original repository to merge your changes.

## Coding Standards

Please follow our coding standards to ensure consistency in the codebase:
- Use meaningful variable names and comments.
- Follow the Chromascope Style Guide.
- Ensure your code passes all tests before submitting a pull request.

## Review and Merge Process

All pull requests will be reviewed by a maintainer. Please ensure that your code:
- Adheres to our coding standards.
- Passes all tests.
- Includes any necessary documentation updates.
- Gives every new control in the window the MCP tool that does the same: the tool, its entry in
  `src/gui/actions.rs` and a test that calls it over the MCP protocol. A feature is not finished
  without them, and `cargo test --features mcp` (`tests/mcp_parity.rs`) fails until they exist.
- Updates the user manual and the MCP guide in the same commit when it changes the window or the
  MCP tools (the pages are in `website/content`). Bold text in the pages is reserved for labels
  that the window shows, and every tool, prompt, resource address and launch flag the MCP guide
  names must exist in the servers; `cargo test --features mcp --test website` checks both, and
  `mkdocs build --strict -f website/mkdocs.yml` (the Website workflow) fails on a broken link.
  Build the site locally with `pip install -r website/requirements.txt`, then run that command
  from the repository root.

Once your pull request is reviewed and approved, it will be merged into the main branch.

## Community and Behavioral Expectations

We strive to create a welcoming and inclusive community. Please be respectful and considerate in your interactions. Follow our Code of Conduct to ensure a positive experience for everyone.
