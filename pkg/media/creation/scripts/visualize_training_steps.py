# This is largely written by Gemini
# https://gemini.google.com/app/a655047c86698dd8

import os
import glob
import time
import pandas as pd
import matplotlib.pyplot as plt
import numpy as np
from multiprocessing import Pool
from moviepy import ImageSequenceClip
from tqdm import tqdm

# Composed Video is 3840 x 2160

## - Plot 1: Visualize data collection over time.
# VIDEO_WIDTH = 1536 / 2
# VIDEO_HEIGHT = 2160 / 2
# MODE = 'data_collection'
# DATA_COLLECTION_CSV = 'data/bed/bed_in_frame.csv'
# OUTPUT_VIDEO_FILENAME = 'data/bed/bed_in_frame_sliding_vis.mp4'
# SLIDING_WINDOW_SECONDS = 60 * 4
# DATA_SELECTION_BUFFER_SECONDS = 2.0
# FRAME_TIME_INTERVAL = 2.0

## - Plot 2: Mainly I need the final frame of the first graph without any sliding window.
# VIDEO_WIDTH = 3840 / 2
# VIDEO_HEIGHT = 2160 / 2
# MODE = 'data_collection'
# DATA_COLLECTION_CSV = 'data/bed/bed_in_frame.csv'
# OUTPUT_VIDEO_FILENAME = 'data/bed/bed_in_frame_full_frame_vis.mp4'
# SLIDING_WINDOW_SECONDS = None
# DATA_SELECTION_BUFFER_SECONDS = 2.0
# FRAME_TIME_INTERVAL = 10.0

## - Plot 3: Training
# VIDEO_WIDTH = 3840 / 2
# VIDEO_HEIGHT = 2160 / 2
# MODE = 'training'
# CSV_FOLDER = 'data/bed/in_frame/training_steps'
# OUTPUT_VIDEO_FILENAME = 'data/bed/bed_in_frame_training_vis.mp4'
# WEIGHTS_CSV_FILE = 'data/bed/in_frame/training_weights.csv'
# TARGET_CSV_FILE = 'data/bed/bed_in_frame.csv'
# TRAINING_FRAME_STRIDE = 10

## - Plot 4: Control Training (Heat Up)
# VIDEO_WIDTH = 3840 / 2
# VIDEO_HEIGHT = 2160 / 2
# MODE = 'training'
# CSV_FOLDER = 'data/bed/in_frame/control_to_100_steps'
# OUTPUT_VIDEO_FILENAME = 'data/bed/bed_in_frame_control_to_100_training_vis.mp4'
# WEIGHTS_CSV_FILE = None
# TARGET_CSV_FILE = None
# TRAINING_FRAME_STRIDE = 1

## - Plot 4: Control Training (Cool Down)
# VIDEO_WIDTH = 3840 / 2
# VIDEO_HEIGHT = 2160 / 2
# MODE = 'training'
# CSV_FOLDER = 'data/bed/in_frame/control_to_0_steps'
# OUTPUT_VIDEO_FILENAME = 'data/bed/bed_in_frame_control_to_0_training_vis.mp4'
# WEIGHTS_CSV_FILE = None
# TARGET_CSV_FILE = None
# TRAINING_FRAME_STRIDE = 1

## - Plot 5: Driving the Bed (to 100C)
VIDEO_WIDTH = 1536 / 2
VIDEO_HEIGHT = 2160 / 2
MODE = 'data_collection'
DATA_COLLECTION_CSV = 'data/bed/in_frame/control_to_100_data_take4.csv'
OUTPUT_VIDEO_FILENAME = 'data/bed/bed_in_frame_control_to_100_data_take4_vis.mp4'
SLIDING_WINDOW_SECONDS = 60 * 4
DATA_SELECTION_BUFFER_SECONDS = 2.0
FRAME_TIME_INTERVAL = 2.0


# --- Main Configuration ---
# Choose the mode: 'training' or 'data_collection'
# MODE = 'training'
NUM_WORKERS = 4
VIDEO_FPS = 15

# --- Mode-Specific Configurations ---
# if MODE == 'training':
#     # --- Training Mode: Analyzes multiple CSVs from a folder ---
#     CSV_FOLDER = 'data/bed/in_frame/training_steps'
#     OUTPUT_VIDEO_FILENAME = 'training_visualization.mp4'
#     # Optional files for training mode
#     WEIGHTS_CSV_FILE = 'data/bed/in_frame/training_weights.csv'
#     TARGET_CSV_FILE = 'data/bed/bed_in_frame.csv'
# elif MODE == 'data_collection':
#     # --- Data Collection Mode: Animates a single CSV over time ---
#     DATA_COLLECTION_CSV = 'data/bed/bed_in_frame.csv'
#     OUTPUT_VIDEO_FILENAME = 'data_collection_timelapse.mp4'
#     SLIDING_WINDOW_SECONDS = 60 * 4
#     # --- NEW: Buffer to select extra data for seamless plotting ---
#     DATA_SELECTION_BUFFER_SECONDS = 2.0

# --- Resolution Configuration ---
# VIDEO_WIDTH = 1920
# VIDEO_HEIGHT = 1080
VIDEO_DPI = 100

# ==============================================================================
# ===== TRAINING MODE FUNCTIONS ================================================
# ==============================================================================

def generate_frame_data_training(args):
    """
    Worker function for 'training' mode. Processes a single CSV file.
    """
    file_path, weights_row, target_df = args['file_path'], args['weights_row'], args['target_df']
    try:
        fig_width_inches = VIDEO_WIDTH / VIDEO_DPI
        fig_height_inches = VIDEO_HEIGHT / VIDEO_DPI
        df = pd.read_csv(file_path)
        fig, ax1 = plt.subplots(figsize=(fig_width_inches, fig_height_inches), dpi=VIDEO_DPI)

        bottom = 0.06
        if weights_row is not None:
            bottom = 0.15

        fig.subplots_adjust(left=0.1, right=0.90, bottom=bottom, top=0.92)

        if target_df is not None:
            ax1.plot(target_df['time'], target_df['bed'], color='red', linestyle='--', label='Bed Target', alpha=0.2)
            ax1.plot(target_df['time'], target_df['sheet'], color='orange', linestyle='--', label='Sheet Target', alpha=0.2)

        ax1.set_xlabel('Time (seconds)', fontsize=14, color='black')
        ax1.set_ylabel('Temperature (°C)', fontsize=14, color='black')
        ax1.plot(df['time'], df['bed'], color='red', linestyle='-', label='Bed Temp')
        ax1.plot(df['time'], df['sheet'], color='orange', linestyle='-', label='Sheet Temp')
        ax1.tick_params(axis='y', labelcolor='black')
        ax1.set_ylim(0, 125)
        max_time = df['time'].max()
        ax1.set_xlim(0, max_time)
        ax1.margins(y=0)
        ax1.grid(True, which='both', linestyle='--', linewidth=0.5)

        control_alpha = 1.0
        if target_df is not None:
            control_alpha = 0.2

        ax2 = ax1.twinx()
        ax2.set_ylabel('Duty Cycle (0-1)', fontsize=14, color='black')
        ax2.plot(df['time'], df['heater'], color='blue', linestyle='--', label='Heater', alpha=control_alpha)
        ax2.plot(df['time'], df['fan'], color='cyan', linestyle='--', label='Fan', alpha=control_alpha)
        ax2.tick_params(axis='y', labelcolor='black')
        ax2.set_ylim(-0.04, 1.1)
        ax2.margins(y=0)
        
        step_number = int(os.path.splitext(os.path.basename(file_path))[0])
        plt.title(f"Training Step {step_number}", fontsize=18)
        
        lines1, labels1 = ax1.get_legend_handles_labels()
        lines2, labels2 = ax2.get_legend_handles_labels()
        ax1.legend(lines1 + lines2, labels1 + labels2, loc='upper right')

        if weights_row is not None:
            w0, w1, w2, w3 = weights_row['w0'], weights_row['w1'], weights_row['w2'], weights_row['w3']
            weights_text = f"Weights = [{w0:.3f}, {w1:.3f}, {w2:.3f}, {w3:.3f}]"
            fig.text(0.5, 0.05, weights_text, ha='center', va='center', fontsize=20, color='black')

        fig.canvas.draw()
        rgba_buffer = fig.canvas.buffer_rgba()
        return np.asarray(rgba_buffer)[:, :, :3]
    except Exception as e:
        tqdm.write(f"Could not process file {file_path}. Error: {e}")
        return None
    finally:
        plt.close('all')

def run_training_mode():
    """
    Main function to execute the 'training' mode visualization.
    """
    csv_files = sorted(glob.glob(os.path.join(CSV_FOLDER, '*.csv')))
    if not csv_files: print(f"Error: No CSV files found in '{CSV_FOLDER}'."); return

    if TRAINING_FRAME_STRIDE > 1:
        csv_files = csv_files[::TRAINING_FRAME_STRIDE]
        print(f"Sampling 1 frame every {TRAINING_FRAME_STRIDE} files.")

    weights_df = pd.read_csv(WEIGHTS_CSV_FILE) if WEIGHTS_CSV_FILE and os.path.exists(WEIGHTS_CSV_FILE) else None
    target_df = pd.read_csv(TARGET_CSV_FILE) if TARGET_CSV_FILE and os.path.exists(TARGET_CSV_FILE) else None
    
    job_args = []
    for file_path in csv_files:
        weights_row = None
        if weights_df is not None:
            try:
                idx = int(os.path.splitext(os.path.basename(file_path))[0])
                if idx < len(weights_df): weights_row = weights_df.iloc[idx]
            except (ValueError, IndexError): pass
        job_args.append({'file_path': file_path, 'weights_row': weights_row, 'target_df': target_df})

    print(f"Running TRAINING mode for {len(csv_files)} files...")
    frames, last_preview_time = [], time.time()
    with Pool(processes=NUM_WORKERS) as pool:
        for result in tqdm(pool.imap(generate_frame_data_training, job_args), total=len(job_args)):
            if result is not None:
                frames.append(result)
                current_time = time.time()
                if (len(frames) == 1) or (current_time - last_preview_time > 5):
                    tqdm.write("✨ Updating preview image: preview.png")
                    plt.imsave("preview.png", result)
                    last_preview_time = current_time
    
    if frames:
        print(f"\nGenerated {len(frames)} frames. Compiling video...")
        clip = ImageSequenceClip(frames, fps=VIDEO_FPS)
        clip.write_videofile(OUTPUT_VIDEO_FILENAME, codec='libx264', logger='bar')
        print(f"\n✅ Successfully created video: {OUTPUT_VIDEO_FILENAME}")

# ==============================================================================
# ===== DATA COLLECTION MODE FUNCTIONS =========================================
# ==============================================================================

def generate_frame_data_collection(args):
    """
    Worker function for 'data_collection' mode. Renders data up to a specific end time.
    """
    full_df, end_time, max_time, window_size = args['full_df'], args['end_time'], args['max_time'], args['window_size']
    try:
        # Determine the start time for the VISIBLE plot window
        plot_start_time = 0
        if window_size is not None and end_time > window_size:
            plot_start_time = end_time - window_size
        
        # --- MODIFIED: Select data in a WIDER window than the plot shows ---
        # This grabs a bit of extra data on each side to ensure lines are drawn to the edge.
        data_select_start = max(0, plot_start_time - DATA_SELECTION_BUFFER_SECONDS)
        data_select_end = end_time + DATA_SELECTION_BUFFER_SECONDS
        
        frame_df = full_df[(full_df['time'] >= data_select_start) & (full_df['time'] <= data_select_end)]
        
        fig_width_inches = VIDEO_WIDTH / VIDEO_DPI
        fig_height_inches = VIDEO_HEIGHT / VIDEO_DPI
        fig, ax1 = plt.subplots(figsize=(fig_width_inches, fig_height_inches), dpi=VIDEO_DPI)

        fig.subplots_adjust(left=0.1, right=0.90, bottom=0.06, top=0.98)
        

        ax1.set_xlabel('Time (seconds)', fontsize=14, color='black')
        ax1.set_ylabel('Temperature (°C)', fontsize=14, color='black')
        ax1.plot(frame_df['time'], frame_df['bed'], color='red', linestyle='-', label='Bed Temp')
        ax1.plot(frame_df['time'], frame_df['sheet'], color='orange', linestyle='-', label='Sheet Temp')
        ax1.tick_params(axis='y', labelcolor='black')
        
        # Set the VISIBLE x-axis limits. Matplotlib will clip the extra data.
        if window_size is None:
            ax1.set_xlim(0, max_time)
        else:
            if end_time > window_size:
                ax1.set_xlim(plot_start_time, end_time)
            else:
                ax1.set_xlim(0, window_size)

        ax1.set_ylim(0, 125)
        ax1.margins(y=0)
        ax1.grid(True, which='both', linestyle='--', linewidth=0.5)

        ax2 = ax1.twinx()
        ax2.set_ylabel('Duty Cycle (0-1)', fontsize=14, color='black')
        ax2.plot(frame_df['time'], frame_df['heater'], color='blue', linestyle='--', label='Heater')
        ax2.plot(frame_df['time'], frame_df['fan'], color='cyan', linestyle='--', label='Fan')
        ax2.tick_params(axis='y', labelcolor='black')
        ax2.set_ylim(-0.04, 1.1)
        ax2.margins(y=0)

        lines1, labels1 = ax1.get_legend_handles_labels()
        lines2, labels2 = ax2.get_legend_handles_labels()
        ax1.legend(lines1 + lines2, labels1 + labels2, loc='upper right')

        fig.canvas.draw()
        rgba_buffer = fig.canvas.buffer_rgba()
        return np.asarray(rgba_buffer)[:, :, :3]
    except Exception as e:
        tqdm.write(f"Could not generate frame for time {end_time}. Error: {e}")
        return None
    finally:
        plt.close('all')

def run_data_collection_mode():
    """
    Main function to execute the 'data_collection' mode visualization.
    """
    if not (DATA_COLLECTION_CSV and os.path.exists(DATA_COLLECTION_CSV)):
        print(f"Error: CSV file not found at '{DATA_COLLECTION_CSV}'"); return

    print(f"Loading data from '{DATA_COLLECTION_CSV}'...")
    full_df = pd.read_csv(DATA_COLLECTION_CSV)
    max_time = full_df['time'].max()
    
    frame_end_times = np.arange(0, max_time, FRAME_TIME_INTERVAL)
    job_args = [{'full_df': full_df, 'end_time': t, 'max_time': max_time, 'window_size': SLIDING_WINDOW_SECONDS} for t in frame_end_times]

    print(f"Running DATA COLLECTION mode for {len(job_args)} frames...")
    frames, last_preview_time = [], time.time()
    with Pool(processes=NUM_WORKERS) as pool:
        for result in tqdm(pool.imap(generate_frame_data_collection, job_args), total=len(job_args)):
            if result is not None:
                frames.append(result)
                current_time = time.time()
                if (len(frames) == 1) or (current_time - last_preview_time > 5):
                    tqdm.write("✨ Updating preview image: preview.png")
                    plt.imsave("preview.png", result)
                    last_preview_time = current_time

    if frames:
        print(f"\nGenerated {len(frames)} frames. Compiling video...")
        clip = ImageSequenceClip(frames, fps=VIDEO_FPS)
        clip.write_videofile(OUTPUT_VIDEO_FILENAME, codec='libx264', logger='bar')
        print(f"\n✅ Successfully created video: {OUTPUT_VIDEO_FILENAME}")

# ==============================================================================
# ===== MAIN EXECUTION =========================================================
# ==============================================================================

if __name__ == '__main__':
    if MODE == 'training':
        run_training_mode()
    elif MODE == 'data_collection':
        run_data_collection_mode()
    else:
        print(f"Error: Unknown MODE '{MODE}'. Please choose 'training' or 'data_collection'.")