import sharp from 'sharp';


export default class Image {

	static async read(data: Buffer): Promise<Image> {
		var img = sharp(data);

		var meta = await img.metadata();
	
		// TODO: Would also be useful to be able to grab the original format
		// TODO: In the case of svg's, we will also need to be able to control the rasterization size
		var out = await img.flatten().toColorspace('srgb').raw().toBuffer({ resolveWithObject: true });
		
		let obj = new Image(
			out.data,
			out.info.width,
			out.info.height,
			out.info.channels, // Should always be 3 as we are dropping alpha
			meta.format
		);
		obj.data = out.data;
		obj.width = out.info.width;
		obj.height = out.info.height;
		obj.channels = out.info.channels; 
		obj.format = meta.format;

		return obj;
	}

	private constructor(
		public data: Buffer,
		public width: number,
		public height: number,
		public channels: number,
		public format?: string
	) {}


	public async save(filename: string) {
		var im = sharp(this.data, {
			raw: {
				width: this.width,
				height: this.height,
				channels: this.channels as (1|2|3|4)
			}
		});
	
		await im.toFile(filename);
	}

	public async resize(width: number, height: number): Promise<Image> {
		var im = sharp(this.data, {
			raw: {
				width: this.width,
				height: this.height,
				channels: this.channels as (1|2|3|4)
			}
		});
	
		var out = await im.resize(width, height).toBuffer({ resolveWithObject: true });
		
		let obj = new Image(
			out.data,
			out.info.width,
			out.info.height,
			out.info.channels,
			this.format
		);
		
		return obj;
	}

	public async similarity(other: Image): Promise<number> {
		let self: Image = this;

		if(self.width*self.height < other.width*other.height) {
			other = await other.resize(self.width, self.height);
		}
		else if(self.width*self.height > other.width*other.height) {
			self = await self.resize(other.width, other.height);
		}
	
		if(self.data.length !== self.data.length || self.channels !== other.channels) {
			throw new Error('Resize failed');
		}
	
		var threshold = 5;
	
		var ndiff = 0;
		for(var i = 0; i < self.data.length; i++) {
			if(Math.abs(self.data[i] - other.data[i]) > threshold) {
				ndiff++;
			}
		}
	
		var similarity = (self.data.length - ndiff) / self.data.length; 
		return similarity;
	}


}
