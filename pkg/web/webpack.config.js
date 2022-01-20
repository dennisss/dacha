const path = require('path');

module.exports = (env, argv) => {
    return {
        entry: env.entry,
        mode: 'development',
        module: {
            rules: [
                {
                    test: /\.tsx?$/,
                    use: [
                        {
                            loader: 'ts-loader',
                            options: {
                                configFile: path.resolve(__dirname, 'tsconfig.json'),
                                onlyCompileBundledFiles: true
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
