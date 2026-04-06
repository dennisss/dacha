import argparse
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter
from moviepy import ImageSequenceClip
from multiprocessing import Pool
import os
import sys
import tempfile
from tqdm import tqdm

VIDEO_DPI = 100
FPS = 30

def hour_formatter(x, pos):
    h = int(x)
    m = int((x - h) * 60)
    return f"{h:02d}:{m:02d}"

def _worker_plot_frame(args):
    df = args['df']
    video_width = args['video_width']
    video_height = args['video_height']
    frame_path = args.get('frame_path')
    current_hour_anim = args['current_hour_anim']
    
    plot_df = df[df['hour'] <= current_hour_anim]
    
    fig_width_inches = video_width / VIDEO_DPI
    fig_height_inches = video_height / VIDEO_DPI
    fig, ax1 = plt.subplots(figsize=(fig_width_inches, fig_height_inches), dpi=VIDEO_DPI)
    ax2 = ax1.twinx()
    
    color1 = 'tab:red'
    ax1.set_xlabel('Hour of Day', fontsize=24)
    ax1.set_ylabel('Temperature (°C)', color=color1, fontsize=24)
    ax1.tick_params(axis='y', labelcolor=color1, labelsize=20)
    ax1.tick_params(axis='x', labelsize=20)
    
    color2 = 'tab:blue'
    ax2.set_ylabel('Humidity (RH %)', color=color2, fontsize=24)
    ax2.tick_params(axis='y', labelcolor=color2, labelsize=20)
    
    if not plot_df.empty:
        for date, group in plot_df.groupby(plot_df['dt'].dt.date):
            ax1.plot(group['hour'], group['temp_smooth'], color=color1, linewidth=3, alpha=0.8)
            ax2.plot(group['hour'], group['humid_smooth'], color=color2, linewidth=3, alpha=0.8)
        
    ax1.set_xlim(0, 24)
    ax1.set_ylim(args['temp_min'], args['temp_max'])
    ax2.set_ylim(args['humid_min'], args['humid_max'])
    
    ax1.xaxis.set_major_formatter(FuncFormatter(hour_formatter))
    ax1.set_xticks(np.arange(0, 25, 3))
    ax1.grid(True, linestyle='--', alpha=0.6)
    
    fig.tight_layout()
    
    if frame_path:
        plt.savefig(frame_path)
        plt.close(fig)
        return frame_path
    else:
        fig.canvas.draw()
        rgba_buffer = fig.canvas.buffer_rgba()
        frame = np.asarray(rgba_buffer)[:, :, :3]
        plt.close(fig)
        return frame

def generate_video(df, output_path, output_width, output_height):
    duration_sec = 1.0
    total_frames = int(duration_sec * FPS)
    hours = np.linspace(0, 24.0, total_frames)
    
    temp_min = df['temp_smooth'].min()
    temp_max = df['temp_smooth'].max()
    t_pad = (temp_max - temp_min) * 0.05
    if t_pad == 0: t_pad = 1.0
    t_limits = (temp_min - t_pad, temp_max + t_pad)
    
    humid_min = df['humid_smooth'].min()
    humid_max = df['humid_smooth'].max()
    h_pad = (humid_max - humid_min) * 0.05
    if h_pad == 0: h_pad = 1.0
    h_limits = (humid_min - h_pad, humid_max + h_pad)
    
    with tempfile.TemporaryDirectory() as temp_dir:
        tasks = []
        for i, h in enumerate(hours):
            frame_path = os.path.join(temp_dir, f"frame_{i:06d}.png")
            tasks.append({
                'df': df,
                'current_hour_anim': h,
                'temp_min': t_limits[0],
                'temp_max': t_limits[1],
                'humid_min': h_limits[0],
                'humid_max': h_limits[1],
                'video_width': output_width,
                'video_height': output_height,
                'frame_path': frame_path
            })
            
        print(f"Rendering {len(tasks)} frames for video...")
        frame_files = []
        with Pool(processes=os.cpu_count() // 2 or 1) as pool:
            for saved_path in tqdm(pool.imap(_worker_plot_frame, tasks), total=len(tasks)):
                frame_files.append(saved_path)
                
        if frame_files:
            frame_files.extend([frame_files[-1]] * 15)  # 0.5 sec pause at end
            
        print(f"Encoding video {output_path}...")
        clip = ImageSequenceClip(frame_files, fps=FPS)
        clip.write_videofile(output_path, codec='libx264', logger='bar')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description="Visualize HDC log data.")
    parser.add_argument('--input', required=True, help="Input CSV file path")
    parser.add_argument('--output', required=True, help="Output MP4 file path")
    parser.add_argument('--width', type=int, default=1920, help="Output video width in pixels")
    parser.add_argument('--height', type=int, default=1080, help="Output video height in pixels")
    args = parser.parse_args()
    
    print(f"Loading data from {args.input}...")
    try:
        df = pd.read_csv(args.input)
    except FileNotFoundError:
        print(f"Error: File not found {args.input}")
        sys.exit(1)
        
    print("Computing derived fields...")
    df['dt'] = pd.to_datetime(df['time'], unit='s', utc=True).dt.tz_convert('America/Los_Angeles')
    df = df.sort_values('dt')
    df = df.set_index('dt')
    
    # Filter to only the day with the most data points to show a single continuous set of lines
    target_date = df.index.date
    val_counts = pd.Series(target_date).value_counts()
    best_date = val_counts.idxmax()
    print(f"Filtering data to only keep the most complete day: {best_date}")
    df = df[df.index.date == best_date].copy()
    
    print("Smoothing data...")
    df['temp_smooth'] = df['temp'].rolling('5min').mean()
    df['humid_smooth'] = df['humid'].rolling('5min').mean()
    
    df = df.reset_index()
    df['hour'] = df['dt'].dt.hour + df['dt'].dt.minute / 60.0 + df['dt'].dt.second / 3600.0
    
    generate_video(df, args.output, args.width, args.height)
