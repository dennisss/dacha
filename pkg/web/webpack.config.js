const path = require('path');
const MiniCssExtractPlugin = require("mini-css-extract-plugin");

module.exports = (env, argv) => {
    return {
        entry: env.entry,
        mode: 'development',
        plugins: [new MiniCssExtractPlugin({
            filename: 'app.css'
        })],
        module: {
            rules: [
                {
                    test: /\.css$/,
                    use: [MiniCssExtractPlugin.loader, 'css-loader']
                },
                {
                    test: /\.tsx?$/,
                    use: [
                        {
                            loader: 'ts-loader',
                            options: {
                                configFile: path.resolve(__dirname, 'tsconfig.json'),
                                onlyCompileBundledFiles: true,
                                transpileOnly: true
                            }
                        }
                    ],
                    exclude: /node_modules/,
                },
            ],
        },
        resolve: {
            // NOTE: Should match tsconfig's 'compilerOptions.paths'
            modules: [
                path.resolve(__dirname, '../../'),
                'node_modules',
            ],
            extensions: ['.tsx', '.ts', '.js'],
        },
        output: {
            filename: path.basename(env.output),
            path: path.dirname(env.output)
        },
    };
};
