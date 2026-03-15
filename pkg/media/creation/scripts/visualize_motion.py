"""
# Slow Video

python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/breadboard_motor/sweep_velocity_5v_3/speed-10-accel-0.csv \
    --output dump/slow_actual_vs_target_step.mp4 \
    --width 1920 --height 2160 \
    --type ACTUAL_VS_TARGET_STEP

python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/breadboard_motor/sweep_velocity_5v_3/speed-10-accel-0.csv \
    --output dump/slow_actual_vs_target_step.png \
    --width 1920 --height 2160 \
    --type ACTUAL_VS_TARGET_STEP

# Fast Video

python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/breadboard_motor/sweep_velocity_5v/speed-80-accel-0.csv \
    --output dump/skipping_actual_vs_target_step.mp4 \
    --width 1920 --height 2160 \
    --type ACTUAL_VS_TARGET_STEP


# Non-wrapped raw sensor output video

python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/breadboard_motor/sweep_velocity_5v_3/speed-10-accel-0.csv \
    --output dump/slow_raw_angle_over_time.mp4 \
    --width 1920 --height 2160 \
    --type ANGLE_RAW_VS_TARGET_STEP

# Error over time

python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/breadboard_motor/angle_calibration.csv \
    --output dump/error_over_time.png \
    --width 3840 --height 2160 \
    --type ERROR_OVER_TIME


# Error wrapped

python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/breadboard_motor/angle_calibration.csv \
    --output dump/error_over_angle.png \
    --width 3840 --height 2160 \
    --type ANGLE_RAW_VS_ERROR


# Error over time for each time.

SPEED=40
python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/breadboard_motor/sweep_velocity_5v_3/speed-$SPEED-accel-0.csv \
    --output dump/error_$SPEED.mp4 \
    --width 1920 --height 2160 \
    --type ERROR_OVER_TIME

# Accel vis



python3 pkg/media/creation/scripts/visualize_motion.py \
    --input data/motion_analysis/voron0/sweep_accel_spreadcycle/speed-600-accel-18000.csv \
    --output dump/accel.png \
    --width 3840 --height 2160 \
    --type ERROR_OVER_TIME

"""

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

# Constants
VIDEO_DPI = 100
FPS = 30

FIT_EVAL_HARMONICS = 4

FONT_SIZE_AXES_LABELS = 24
FONT_SIZE_TICK_LABELS = 20
FONT_SIZE_LEGEND = 20

GRAPH_TYPES = ['ACTUAL_VS_TARGET_STEP', 'ANGLE_RAW_VS_TARGET_STEP', 'ERROR_OVER_TIME', 'TYPE4', 'ANGLE_RAW_VS_ERROR']

def step_formatter(x, pos):
    if abs(x) >= 2000:
        val = x / 1000.0
        return f"{int(val)}K" if int(val) == val else f"{val:.1f}K"
    return f"{int(x)}" if int(x) == x else f"{x:g}"

def compute_fourier_coefficients(x, y, num_harmonics=4):
    n_samples = len(x)
    n_cols = 1 + 2 * num_harmonics
    X = np.ones((n_samples, n_cols)) 
    for k in range(1, num_harmonics + 1):
        theta_rad = 2 * np.pi * k * x
        X[:, 2*k - 1] = np.cos(theta_rad) 
        X[:, 2*k]     = np.sin(theta_rad) 
    
    X_T = X.T
    try:
        beta = np.linalg.inv(X_T @ X) @ X_T @ y
        return beta
    except np.linalg.LinAlgError:
        print("Warning: Matrix inversion failed. Returning zero coefficients.")
        return np.zeros(n_cols)

def evaluate_fourier(x, beta, num_harmonics=FIT_EVAL_HARMONICS):
    A0 = beta[0]
    y = np.full_like(x, A0, dtype=float)
    for k in range(1, num_harmonics + 1):
        theta_rad = 2 * np.pi * k * x
        Ak = beta[2*k - 1]
        Bk = beta[2*k]
        y += Ak * np.cos(theta_rad) + Bk * np.sin(theta_rad)
    return y

def _worker_plot_frame(args):
    """
    Multiprocessing worker function.
    args = {
        'df': DataFrame,
        'current_time': float,
        'graph_type': str,
        'x_limits': tuple,
        'y_limits': tuple,
        'fourier_beta': np.array,
        'video_width': int,
        'video_height': int,
        'frame_path': str  # Optional path to save frame dynamically
    }
    """
    df = args['df']
    current_time = args['current_time']
    graph_type = args['graph_type']
    video_width = args['video_width']
    video_height = args['video_height']
    frame_path = args.get('frame_path')
    
    if current_time is not None:
        plot_df = df[df['time'] <= current_time]
    else:
        plot_df = df

    fig_width_inches = video_width / VIDEO_DPI
    fig_height_inches = video_height / VIDEO_DPI
    fig, ax = plt.subplots(figsize=(fig_width_inches, fig_height_inches), dpi=VIDEO_DPI)
    
    if graph_type == 'ACTUAL_VS_TARGET_STEP':
        ax.plot(plot_df['time'], plot_df['target_step'], label='Target Step', color='blue', alpha=0.4, linewidth=6, zorder=1)
        ax.plot(plot_df['time'], plot_df['actual_step'], label='Actual Step', color='red', linewidth=1.5, zorder=2)
        ax.set_xlabel('Time (s)', fontsize=FONT_SIZE_AXES_LABELS)
        ax.set_ylabel('Steps', fontsize=FONT_SIZE_AXES_LABELS)
        ax.legend(fontsize=FONT_SIZE_LEGEND)
    elif graph_type == 'ANGLE_RAW_VS_TARGET_STEP':
        ax.plot(plot_df['time'], plot_df['target_step'], label='Target Step', color='blue', alpha=0.4, linewidth=6, zorder=1)
        ax.plot(plot_df['time'], plot_df['angle_raw_step'], label='Angle Raw (Step units)', color='red', linewidth=1.5, zorder=2)
        ax.set_xlabel('Time (s)', fontsize=FONT_SIZE_AXES_LABELS)
        ax.set_ylabel('Steps', fontsize=FONT_SIZE_AXES_LABELS)
        ax.legend(fontsize=FONT_SIZE_LEGEND)
    elif graph_type == 'ERROR_OVER_TIME':
        ax.plot(plot_df['time'], plot_df['error_step'], label='Error (Step units)', color='red', linewidth=1.5, zorder=2)
        ax.set_xlabel('Time (s)', fontsize=FONT_SIZE_AXES_LABELS)
        ax.set_ylabel('Steps', fontsize=FONT_SIZE_AXES_LABELS)
        ax.legend(fontsize=FONT_SIZE_LEGEND)
    elif graph_type == 'TYPE4':
        ax.scatter(plot_df['angle_raw'], plot_df['error_step'], s=2, color='gray', alpha=0.5)
        ax.set_xlabel('Angle Raw (0 to 1)', fontsize=FONT_SIZE_AXES_LABELS)
        ax.set_ylabel('Error (Step units)', fontsize=FONT_SIZE_AXES_LABELS)
    elif graph_type == 'ANGLE_RAW_VS_ERROR':
        ax.scatter(plot_df['angle_raw'], plot_df['error_step'], s=2, color='red', alpha=1, label='Error Points')
        if args['fourier_beta'] is not None:
            x_fit = np.linspace(0, 1.0, 1000)
            y_fit = evaluate_fourier(x_fit, args['fourier_beta'])
            ax.plot(x_fit, y_fit, color='blue', linewidth=2.5, label='Best Fit')
        ax.set_xlabel('Angle Raw (0 to 1)', fontsize=FONT_SIZE_AXES_LABELS)
        ax.set_ylabel('Error (Step units)', fontsize=FONT_SIZE_AXES_LABELS)
        ax.legend(fontsize=FONT_SIZE_LEGEND)

    if args['x_limits']:
        ax.set_xlim(args['x_limits'])
    if args['y_limits']:
        ax.set_ylim(args['y_limits'])
        
    ax.yaxis.set_major_formatter(FuncFormatter(step_formatter))
    ax.tick_params(axis='both', which='major', labelsize=FONT_SIZE_TICK_LABELS)
    ax.grid(True, linestyle='--', alpha=0.6)
    
    # Optional: ensure layout doesn't clip titles/labels
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

def generate_video(df, graph_type, output_path, limits, fourier_beta, output_width, output_height):
    max_time = df['time'].max()
    times = list(np.arange(0, max_time, 1.0 / FPS))
    if not times or times[-1] < max_time - 1e-5:
        times.append(max_time)
    
    with tempfile.TemporaryDirectory() as temp_dir:
        tasks = []
        for i, t in enumerate(times):
            frame_path = os.path.join(temp_dir, f"frame_{i:06d}.png")
            tasks.append({
                'df': df,
                'current_time': t,
                'graph_type': graph_type,
                'x_limits': limits.get('x'),
                'y_limits': limits.get('y'),
                'fourier_beta': fourier_beta,
                'video_width': output_width,
                'video_height': output_height,
                'frame_path': frame_path
            })
            
        print(f"Rendering {len(tasks)} frames for video...")
        frame_files = []
        # Avoid too many workers consuming all RAM for matplotlib
        with Pool(processes=os.cpu_count() // 2 or 1) as pool:
            for saved_path in tqdm(pool.imap(_worker_plot_frame, tasks), total=len(tasks)):
                frame_files.append(saved_path)
                
        if frame_files:
            # MoviePy float math sometimes truncates the final frame during encoding.
            # Duplicating it ensures the final state is represented and adds a brief pause.
            frame_files.extend([frame_files[-1]] * 5)
                    
        print(f"Encoding video {output_path}...")
        clip = ImageSequenceClip(frame_files, fps=FPS)
        clip.write_videofile(output_path, codec='libx264', logger='bar')

def generate_image(df, graph_type, output_path, limits, fourier_beta, output_width, output_height):
    print(f"Rendering single image for {output_path}...")
    args = {
        'df': df,
        'current_time': None,
        'graph_type': graph_type,
        'x_limits': limits.get('x'),
        'y_limits': limits.get('y'),
        'fourier_beta': fourier_beta,
        'video_width': output_width,
        'video_height': output_height,
        'frame_path': None
    }
    frame = _worker_plot_frame(args)
    plt.imsave(output_path, frame)
    print(f"Saved image to {output_path}")

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description="Visualize motion capture data.")
    parser.add_argument('--input', required=True, help="Input CSV file path")
    parser.add_argument('--output', required=True, help="Output file path (PNG or MP4)")
    parser.add_argument('--type', required=True, choices=GRAPH_TYPES, help="Graph type placeholder (e.g. ACTUAL_VS_TARGET_STEP)")
    parser.add_argument('--width', type=int, default=1920, help="Output image/video width in pixels")
    parser.add_argument('--height', type=int, default=1080, help="Output image/video height in pixels")
    args = parser.parse_args()
    
    ext = os.path.splitext(args.output)[1].lower()
    is_video = (ext == '.mp4')
    
    print(f"Loading data from {args.input}...")
    try:
        df = pd.read_csv(args.input)
    except FileNotFoundError:
        print(f"Error: File not found {args.input}")
        sys.exit(1)
        
    print("Computing derived fields...")
    df['angle_raw_step'] = df['angle_raw'] * 3200
    df['error_step'] = df['error'] * 3200
    
    # Subsampling to manage dense 8kHz data
    downsample_factor = 40 if is_video else 20
    print(f"Subsampling data by factor of {downsample_factor}...")
    # Using iloc instead of rolling mean to avoid artifacts around 0/1 wraparounds for angles
    df_plot = df.iloc[::downsample_factor].copy()
    
    limits = {'x': None, 'y': None}
    fourier_beta = None
    
    # Calculate global limits to keep plots stable
    if args.type in ['ACTUAL_VS_TARGET_STEP', 'ANGLE_RAW_VS_TARGET_STEP', 'ERROR_OVER_TIME']:
        limits['x'] = (0, df_plot['time'].max())
        if args.type == 'ACTUAL_VS_TARGET_STEP':
            min_y = min(df_plot['actual_step'].min(), df_plot['target_step'].min())
            max_y = max(df_plot['actual_step'].max(), df_plot['target_step'].max())
        elif args.type == 'ANGLE_RAW_VS_TARGET_STEP':
            min_y = min(df_plot['angle_raw_step'].min(), df_plot['target_step'].min())
            max_y = max(df_plot['angle_raw_step'].max(), df_plot['target_step'].max())
        elif args.type == 'ERROR_OVER_TIME':
            # min_y = df_plot['error_step'].min()
            # max_y = df_plot['error_step'].max()

            min_y = 0
            max_y = 20

        y_range = max_y - min_y
        padding = y_range * 0.05 if y_range != 0 else 1.0
        limits['y'] = (min_y - padding, max_y + padding)
    else:
        limits['x'] = (0, 1.0)
        min_y = df_plot['error_step'].min()
        max_y = df_plot['error_step'].max()
        y_range = max_y - min_y
        padding = y_range * 0.05 if y_range != 0 else 1.0
        limits['y'] = (min_y - padding, max_y + padding)
        
        if args.type == 'ANGLE_RAW_VS_ERROR':
            print("Computing Fourier coefficients for regression...")
            fourier_beta = compute_fourier_coefficients(df_plot['angle_raw'].values, df_plot['error_step'].values)

    if is_video:
        generate_video(df_plot, args.type, args.output, limits, fourier_beta, args.width, args.height)
    else:
        generate_image(df_plot, args.type, args.output, limits, fourier_beta, args.width, args.height)
