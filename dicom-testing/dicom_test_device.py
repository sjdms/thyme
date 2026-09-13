#!/usr/bin/env python3
"""Interactive DICOM test-device simulator for Thyme's DICOM receiver
(``server::dicom``).

Lets you send a C-ECHO, a synthetic autorefraction reading (the kind an
auto-refractor/keratometer reports), or a synthetic OCT image to
Thyme's DICOM listener over the network, without needing any real
hardware. Every value the receiver actually reads (see
``server/src/dicom/tags.rs``) is something you're prompted for; nothing
here is faked "behind the scenes."

Requires:
    pip install pydicom pynetdicom

Usage:
    python3 dicom_test_device.py
"""

from __future__ import annotations

import datetime
import os
import sys

from pydicom.dataset import Dataset, FileMetaDataset
from pydicom.uid import ImplicitVRLittleEndian, generate_uid
from pynetdicom import AE
from pynetdicom.sop_class import (
    AutorefractionMeasurementsStorage,
    OphthalmicTomographyImageStorage,
    Verification,
)

# -- small input helpers -----------------------------------------------


def ask(prompt: str, default: str | None = None) -> str | None:
    suffix = f" [{default}]" if default is not None else ""
    raw = input(f"{prompt}{suffix}: ").strip()
    return raw if raw else default


def ask_required(prompt: str, default: str | None = None) -> str:
    while True:
        value = ask(prompt, default)
        if value:
            return value
        print("  A value is required.")


def ask_float(prompt: str, default: str | None = None, allow_blank: bool = False) -> float | None:
    while True:
        raw = ask(prompt, default)
        if not raw:
            if allow_blank:
                return None
            print("  A value is required.")
            continue
        try:
            return float(raw)
        except ValueError:
            print("  Not a number — try again.")


def ask_int(prompt: str, default: int) -> int:
    while True:
        raw = ask(prompt, str(default))
        try:
            return int(raw)
        except (TypeError, ValueError):
            print("  Not a whole number — try again.")


def ask_yes_no(prompt: str, default: bool = True) -> bool:
    hint = "Y/n" if default else "y/N"
    raw = input(f"{prompt} ({hint}): ").strip()
    if not raw:
        return default
    return raw.strip().lower().startswith("y")


# -- dataset construction ------------------------------------------------
#
# Every tag set below matches `server/src/dicom/tags.rs` exactly — that
# file is effectively this script's spec sheet. Both SOP classes are
# sent as Implicit VR Little Endian, one of the two transfer syntaxes
# `server::dicom::association` negotiates.


def new_dataset(sop_class_uid: str, patient_id: str, patient_name: str, modality: str) -> Dataset:
    ds = Dataset()
    sop_instance_uid = generate_uid()

    ds.SOPClassUID = sop_class_uid
    ds.SOPInstanceUID = sop_instance_uid
    ds.StudyInstanceUID = generate_uid()
    ds.SeriesInstanceUID = generate_uid()
    ds.Modality = modality
    ds.PatientID = patient_id
    ds.PatientName = patient_name

    ds.file_meta = FileMetaDataset()
    ds.file_meta.MediaStorageSOPClassUID = sop_class_uid
    ds.file_meta.MediaStorageSOPInstanceUID = sop_instance_uid
    ds.file_meta.TransferSyntaxUID = ImplicitVRLittleEndian
    ds.is_little_endian = True
    ds.is_implicit_VR = True
    return ds


def ask_patient_id() -> str:
    print(
        "  Patient ID this instance carries (DICOM tag 0010,0020) — "
        "server::dicom::patient_match treats this as the clinic's MRN."
    )
    return ask_required(
        "  Enter a real Thyme MRN to test a successful match, or anything else to test 'unmatched'"
    )


def build_refraction_eye(label: str) -> Dataset | None:
    if not ask_yes_no(f"Measure the {label} eye?", True):
        return None

    sphere = ask_float(f"  {label} sphere power, diopters (e.g. -2.25)")
    cylinder = ask_float(f"  {label} cylinder power, diopters (blank = no astigmatism measured)", allow_blank=True)
    axis = ask_float(f"  {label} cylinder axis, degrees (0-180)") if cylinder is not None else None

    item = Dataset()
    item.SpherePower = sphere
    if cylinder is not None:
        cylinder_item = Dataset()
        cylinder_item.CylinderPower = cylinder
        cylinder_item.CylinderAxis = axis
        item.CylinderSequence = [cylinder_item]
    return item


def build_refraction_dataset() -> Dataset:
    print("\n--- Autorefraction reading ---")
    patient_id = ask_patient_id()
    patient_name = ask("Patient name (DICOM PN format, e.g. DOE^JANE)", "TEST^PATIENT")

    ds = new_dataset(AutorefractionMeasurementsStorage, patient_id, patient_name, "OPTR")

    right = build_refraction_eye("right (OD)")
    left = build_refraction_eye("left (OS)")

    if right is None and left is None:
        print(
            "  Neither eye measured — server::dicom::refraction stages nothing for an "
            "instance like this, so this send will succeed at the network level but "
            "won't create anything to review."
        )
    if right is not None:
        ds.AutorefractionRightEyeSequence = [right]
    if left is not None:
        ds.AutorefractionLeftEyeSequence = [left]

    return ds


def build_oct_dataset() -> Dataset:
    print("\n--- OCT image ---")
    patient_id = ask_patient_id()
    patient_name = ask("Patient name (DICOM PN format, e.g. DOE^JANE)", "TEST^PATIENT")
    eye = ask("Eye — Image Laterality (OD/OS/OU, blank = not recorded)", "OD")

    rows = ask_int("Rows", 512)
    columns = ask_int("Columns", 512)
    frames = ask_int("Number of frames", 1)

    ds = new_dataset(OphthalmicTomographyImageStorage, patient_id, patient_name, "OPT")
    if eye:
        ds.ImageLaterality = eye
    ds.AcquisitionDate = datetime.date.today().strftime("%Y%m%d")
    ds.AcquisitionTime = datetime.datetime.now().strftime("%H%M%S")

    # Enough of the Image Pixel module to be a valid image, not just
    # what `server::dicom::oct` happens to read — Rows/Columns/
    # NumberOfFrames/PixelData are the ones it actually looks at.
    ds.SamplesPerPixel = 1
    ds.PhotometricInterpretation = "MONOCHROME2"
    ds.Rows = rows
    ds.Columns = columns
    ds.NumberOfFrames = frames
    ds.BitsAllocated = 16
    ds.BitsStored = 16
    ds.HighBit = 15
    ds.PixelRepresentation = 0

    byte_count = rows * columns * frames * 2
    print(f"  Generating {byte_count:,} bytes of synthetic (random noise) pixel data...")
    # Content doesn't matter for testing ingestion — only that it's the
    # right number of bytes for Rows x Columns x NumberOfFrames at 16
    # bits/pixel, which is what `oct_images` stores for later.
    ds.PixelData = os.urandom(byte_count)

    return ds


# -- sending ---------------------------------------------------------------


def associate(host: str, port: int, called_ae: str, calling_ae: str, abstract_syntax: str):
    ae = AE(ae_title=calling_ae)
    ae.add_requested_context(abstract_syntax)
    print(f"\nAssociating with {host}:{port} (calling AE {called_ae!r})...")
    assoc = ae.associate(host, port, ae_title=called_ae)
    if not assoc.is_established:
        print(
            "  Association was not established. Check the host/port/AE titles, and that "
            "server::dicom::DicomScp is actually running and listening there."
        )
        return None
    return assoc


def send_echo(host: str, port: int, called_ae: str, calling_ae: str) -> None:
    assoc = associate(host, port, called_ae, calling_ae, Verification)
    if assoc is None:
        return
    try:
        status = assoc.send_c_echo()
        if status and getattr(status, "Status", None) == 0x0000:
            print("  C-ECHO succeeded — the server is reachable and negotiation works.")
        else:
            print(f"  C-ECHO returned an unexpected status: {status}")
    finally:
        assoc.release()


def send_store(ds: Dataset, host: str, port: int, called_ae: str, calling_ae: str) -> None:
    assoc = associate(host, port, called_ae, calling_ae, ds.SOPClassUID)
    if assoc is None:
        return
    try:
        status = assoc.send_c_store(ds)
        if status is None:
            print("  No response received — the association may have aborted mid-transfer.")
        elif status.Status == 0x0000:
            print(f"  C-STORE succeeded (SOP Instance UID {ds.SOPInstanceUID}).")
            print(
                "  Check `device_studies`/`oct_images`/`device_suggested_actions` in the "
                "database, or the patient's file in the client, to see where it landed."
            )
        else:
            print(f"  C-STORE returned status 0x{status.Status:04X} (not Success).")
    finally:
        assoc.release()


# -- main loop ---------------------------------------------------------------

MENU = """
What would you like to test?
  1) C-ECHO          - just verify the server is reachable
  2) Autorefraction  - a refraction reading (auto-refractor/keratometer-style)
  3) OCT image       - an imaging study
  4) Quit
"""


def ask_connection_settings():
    print("--- Server connection ---")
    host = ask("Server host/IP", "127.0.0.1")
    port = ask_int("Server port", 11112)
    called_ae = ask("Server's Called AE Title (matches DICOM_AE_TITLE)", "THYME-SCP")
    calling_ae = ask("This test device's Calling AE Title", "TEST-DEVICE")
    return host, port, called_ae, calling_ae


def main() -> None:
    print("Thyme DICOM test-device simulator")
    print("Sends synthetic DICOM instances to server::dicom — no real hardware needed.\n")

    host, port, called_ae, calling_ae = ask_connection_settings()

    while True:
        print(MENU)
        choice = ask("Choice", "2")

        if choice == "1":
            send_echo(host, port, called_ae, calling_ae)
        elif choice == "2":
            send_store(build_refraction_dataset(), host, port, called_ae, calling_ae)
        elif choice == "3":
            send_store(build_oct_dataset(), host, port, called_ae, calling_ae)
        elif choice == "4":
            break
        else:
            print("Not a valid choice — enter 1, 2, 3, or 4.")
            continue

        if not ask_yes_no("\nRun another test?", True):
            break

    print("Done.")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nInterrupted.")
        sys.exit(1)
