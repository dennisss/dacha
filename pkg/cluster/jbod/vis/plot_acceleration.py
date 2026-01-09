# Written by Gemini
# https://gemini.google.com/app/d4d94e36eb970296
#
# python3 pkg/cluster/jbod/vis/plot_acceleration.py
# 
# ls data/hdd_accel/
# random-read-accel.bin   seq-read-accel.bin   static-both-sides-on-accel.bin
# random-write-accel.bin  seq-write-accel.bin  static-left-only-accel.bin

import numpy as np
import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
from scipy import signal
import os
import sys

# ---------------------------------------------------------
# 1. Configuration (EDIT THESE VALUES)
# ---------------------------------------------------------

# INPUT_FILE = "data/hdd_accel/static-both-sides-on-accel.bin"
# PLOT_TITLE = "Idle (Disks Spinning) Vibration"
# OUTPUT_FILENAME = "data/hdd_accel/static-both-sides-on-accel.png"

# INPUT_FILE = "data/hdd_accel/seq-read-accel.bin"
# PLOT_TITLE = "Sequential Reads Vibration"
# OUTPUT_FILENAME = "data/hdd_accel/seq-read-accel.png"

# INPUT_FILE = "data/hdd_accel/seq-write-accel.bin"
# PLOT_TITLE = "Sequential Writes Vibration"
# OUTPUT_FILENAME = 'data/hdd_accel/seq-write-accel.png'

# INPUT_FILE = "data/hdd_accel/random-read-accel.bin"
# PLOT_TITLE = "Random (4K) Reads Vibration"
# OUTPUT_FILENAME = 'data/hdd_accel/random-read-accel.png'

INPUT_FILE = "data/hdd_accel/random-write-accel.bin"
PLOT_TITLE = "Random (4K) Writes Vibration"
OUTPUT_FILENAME = 'data/hdd_accel/random-write-accel.png'


# Image Saving Configuration
SAVE_PLOT = True                  # Set to True to save image
IMG_WIDTH_PX = 1920               # Target Width
IMG_HEIGHT_PX = 1080              # Target Height
DPI = 150                         # Dots Per Inch (Controls text size scaling)

# Signal Processing
FS = 3200             # Sampling rate in Hz
HIGH_PASS_FREQ = 5.0  # UPDATED: Remove gravity/drift below this (Hz)
LOW_PASS_FREQ = 500.0 # Remove noise above this (Hz)
FILTER_ORDER = 4      # Order of the Butterworth filter
SCALE_FACTOR = 0.0039 # Scaling to convert raw integer to Gs

# Visualization Settings
TARGET_POINTS = 800   # Target number of points to plot per axis. 
                      # Lowers visual noise while keeping peaks.

# ---------------------------------------------------------
# 2. Data Loading
# ---------------------------------------------------------
def load_binary_data(filepath):
    if not os.path.exists(filepath):
        print(f"Error: File '{filepath}' not found.")
        sys.exit(1)
    try:
        raw_ints = np.fromfile(filepath, dtype=np.dtype('<i2'), count=-1)
    except Exception as e:
        print(f"Error reading file: {e}")
        sys.exit(1)

    if raw_ints.size % 3 != 0:
        raw_ints = raw_ints[:-(raw_ints.size % 3)]

    data_g = raw_ints.reshape(-1, 3).astype(np.float32) * SCALE_FACTOR
    return data_g

print(f"Loading data from: {INPUT_FILE}")
acceleration_data = load_binary_data(INPUT_FILE)
num_samples = acceleration_data.shape[0]
duration = num_samples / FS

print(f"Loaded {num_samples} samples ({duration:.2f} s).")

# ---------------------------------------------------------
# 3. Processing: Band-Pass Filter (Full Resolution)
# ---------------------------------------------------------
def apply_bandpass_filter(data, fs, low_cut, high_cut, order):
    if data.shape[0] < 30: return data
    nyquist = 0.5 * fs
    low = low_cut / nyquist
    high = high_cut / nyquist
    b, a = signal.butter(order, [low, high], btype='bandpass', analog=False)
    filtered_data = signal.filtfilt(b, a, data, axis=0)
    return filtered_data

print(f"Filtering data (Band-pass: {HIGH_PASS_FREQ}Hz - {LOW_PASS_FREQ}Hz)...")
filtered_accel = apply_bandpass_filter(acceleration_data, FS, HIGH_PASS_FREQ, LOW_PASS_FREQ, FILTER_ORDER)

# ---------------------------------------------------------
# 4. Downsampling (Min-Max Algorithm)
# ---------------------------------------------------------
def downsample_keep_peaks(data, target_points):
    """
    Downsamples data for visualization by keeping the Min and Max 
    of every chunk. This preserves peak values that simple decimation misses.
    """
    n_samples = data.shape[0]
    if n_samples <= target_points:
        return data, np.arange(n_samples)

    # Calculate chunk size (how many samples to compress into 2 points)
    # We produce 2 points (min & max) per chunk, so divide target by 2
    chunk_size = int(n_samples // (target_points / 2))
    
    # Trim data to be a multiple of chunk_size
    n_chunks = n_samples // chunk_size
    trimmed_length = n_chunks * chunk_size
    reshaped = data[:trimmed_length].reshape(n_chunks, chunk_size)
    
    # Find min and max for each chunk
    mins = reshaped.min(axis=1)
    maxs = reshaped.max(axis=1)
    
    # Interleave results: [min, max, min, max...]
    downsampled = np.empty(n_chunks * 2, dtype=data.dtype)
    downsampled[0::2] = mins
    downsampled[1::2] = maxs
    
    # Create corresponding time indices
    # We map these points to the beginning and end of their chunks approx
    downsampled_indices = np.linspace(0, trimmed_length-1, n_chunks * 2)
    
    return downsampled, downsampled_indices

print(f"Downsampling for visualization (Target: ~{TARGET_POINTS} points)...")

# Prepare lists to hold plot-ready data
plot_data_list = []
plot_time_list = []

for i in range(3):
    ds_vals, ds_idxs = downsample_keep_peaks(filtered_accel[:, i], TARGET_POINTS)
    plot_data_list.append(ds_vals)
    # Create time array for this specific axis (convert indices to seconds)
    plot_time_list.append(ds_idxs / FS)

# ---------------------------------------------------------
# 5. Plotting
# ---------------------------------------------------------
print("Generating plot...")
fig_width_in = IMG_WIDTH_PX / DPI
fig_height_in = IMG_HEIGHT_PX / DPI
fig, ax = plt.subplots(figsize=(fig_width_in, fig_height_in), dpi=DPI)

colors = ['#1f77b4', '#ff7f0e', '#2ca02c'] # Blue, Orange, Green
labels = ['X Axis', 'Y Axis', 'Z Axis']

for i in range(3):
    # IMPORTANT: Calculate RMS using the FULL RESOLUTION data
    rms_val = np.sqrt(np.mean(filtered_accel[:, i]**2))
    label_text = f"{labels[i]} (RMS: {rms_val:.4f} G)"
    
    # Plot using the DOWNSAMPLED data
    ax.plot(plot_time_list[i], plot_data_list[i], label=label_text, color=colors[i], linewidth=0.8, alpha=0.9)

# Threshold lines
ax.axhline(y=0.2, color='black', linestyle='--', linewidth=1.5, label='0.2 G (Low Performance Limit)')
ax.axhline(y=0.67, color='red', linestyle='--', linewidth=1.5, label='0.67 G (Failure Limit)')
ax.axhline(y=-0.2, color='black', linestyle=':', linewidth=1, alpha=0.3)
ax.axhline(y=-0.67, color='red', linestyle=':', linewidth=1, alpha=0.3)

ax.yaxis.set_major_locator(ticker.MultipleLocator(0.1))
ax.set_title(f"{PLOT_TITLE}\n(5 - 500Hz)")
ax.set_xlabel('Time (s)')
ax.set_ylabel('Acceleration (G)')
ax.legend(loc='upper right', framealpha=0.9)
ax.grid(True, which='both', linestyle='-', alpha=0.3)

plt.tight_layout()

if SAVE_PLOT:
    print(f"Saving high-res image to {OUTPUT_FILENAME} ({IMG_WIDTH_PX}x{IMG_HEIGHT_PX})...")
    plt.savefig(OUTPUT_FILENAME, dpi=DPI, bbox_inches='tight')

print("Displaying plot...")
plt.show()
