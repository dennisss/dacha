

firebase hosting:sites:create madebydennis-com
firebase target:apply hosting madebydennis-com madebydennis-com

firebase hosting:sites:create dacha-dev
firebase target:apply hosting dacha-dev dacha-dev

nvm use
firebase deploy


cargo run --bin source_control -- generate-files-json

firebase deploy --only functions
