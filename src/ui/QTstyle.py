from PySide6.QtGui import QImage, QPalette, QColor, QIcon
from PySide6.QtCore import Qt, QPropertyAnimation, QRect
from PySide6.QtWidgets import QGraphicsOpacityEffect, QWidget


def styleSheet():
    return """
QMainWindow, QDialog {
    background-color: #0f1319;
    color: #e8eef6;
    font-size: 12px;
}

#centralwidget {
    background: qlineargradient(
        x1: 0, y1: 0, x2: 1, y2: 1,
        stop: 0 #111923,
        stop: 0.55 #0f141d,
        stop: 1 #0c1017
    );
}

#leftMenuSubContainer {
    background-color: rgba(14, 24, 36, 0.96);
    border: 1px solid #243244;
    border-radius: 22px;
}

#bottomMenuSubContainer,
QStackedWidget,
QScrollArea,
QFrame {
    background-color: rgba(14, 22, 31, 0.96);
    border: 1px solid #2a3e54;
    border-radius: 16px;
}

QLabel {
    color: #e8eef6;
}

QLineEdit,
QTextEdit,
QPlainTextEdit,
QListWidget,
QComboBox,
QSpinBox,
QDoubleSpinBox {
    background-color: #111a24;
    color: #edf3ff;
    border: 1px solid #2a3a4f;
    border-radius: 10px;
    padding: 7px 10px;
    selection-background-color: #2f8cff;
    selection-color: #ffffff;
}

QLineEdit[echoMode="0"] {
    color: #edf3ff;
}

QLineEdit::placeholder {
    color: #88a0bb;
}

QLineEdit:focus,
QTextEdit:focus,
QPlainTextEdit:focus,
QListWidget:focus,
QComboBox:focus,
QSpinBox:focus,
QDoubleSpinBox:focus {
    border: 1px solid #4aa3ff;
}

QPushButton {
    background-color: #1b2735;
    color: #e9f1ff;
    border: 1px solid #2d4057;
    border-radius: 10px;
    padding: 7px 12px;
}

QPushButton:hover {
    background-color: #223345;
    border-color: #3d5674;
}

QPushButton:pressed {
    background-color: #192838;
}

QPushButton:checked {
    background-color: #2f8cff;
    border: 1px solid #5ca5ff;
    color: #ffffff;
}

QPushButton:disabled {
    background-color: #141d28;
    border-color: #223245;
    color: #70839a;
}

QPushButton[navButton="true"] {
    text-align: left;
    padding: 12px 14px;
    border-radius: 12px;
    font-size: 13px;
    font-weight: 600;
    background-color: #15263a;
    border-color: #2c4562;
}

QCheckBox {
    color: #dce7f7;
}

QCheckBox::indicator {
    width: 14px;
    height: 14px;
    border: 1px solid #5d7998;
    border-radius: 4px;
    background-color: #101722;
}

QCheckBox::indicator:checked {
    image: url(:/icons/icons/check.svg);
    background-color: #2f8cff;
    border-color: #56a0ff;
}

QProgressBar {
    background-color: #131c27;
    border: 1px solid #2a3a4f;
    border-radius: 9px;
    text-align: center;
    color: #dbe8ff;
}

QProgressBar::chunk {
    background: qlineargradient(x1:0,y1:0,x2:1,y2:0,stop:0 #2f8cff, stop:1 #53b2ff);
    border-radius: 8px;
}

QTabWidget::pane {
    border: 1px solid #2a3a4f;
    border-radius: 12px;
    background: #0f1824;
    margin-top: 6px;
}

QTabBar::tab {
    background: #132236;
    color: #d5e3f5;
    border: 1px solid #315072;
    border-radius: 8px;
    padding: 8px 12px;
    margin-right: 6px;
}

QTabBar::tab:selected {
    background: #2f8cff;
    color: #ffffff;
    border-color: #5ca5ff;
}

QTabBar::tab:hover:!selected {
    background: #1d2d40;
}

QScrollBar:vertical {
    width: 9px;
    background: transparent;
}

QScrollBar:horizontal {
    height: 9px;
    background: transparent;
}

QScrollBar::handle:vertical,
QScrollBar::handle:horizontal {
    background: #3f5570;
    border-radius: 4px;
    min-height: 20px;
    min-width: 20px;
}

QScrollBar::handle:vertical:hover,
QScrollBar::handle:horizontal:hover {
    background: #547295;
}

QScrollBar::add-line,
QScrollBar::sub-line {
    width: 0px;
    height: 0px;
    border: none;
    background: transparent;
}

QToolTip {
    background-color: #152131;
    color: #eaf2ff;
    border: 1px solid #3d5674;
    padding: 6px 8px;
    border-radius: 6px;
}

QPushButton[primaryAction="true"] {
    background: qlineargradient(x1:0,y1:0,x2:1,y2:0,stop:0 #1f90ff, stop:1 #2cb2ff);
    border: 1px solid #66b5ff;
    color: #ffffff;
    font-weight: 600;
}

QPushButton[primaryAction="true"]:hover {
    background: qlineargradient(x1:0,y1:0,x2:1,y2:0,stop:0 #2797ff, stop:1 #39b8ff);
    border-color: #8ac6ff;
}

QWidget[rowType="header"] {
    background-color: #13263c;
    border: 1px solid #355478;
    border-radius: 12px;
}

QWidget[rowType="card"] {
    background-color: #0e1722;
    border: 1px solid #2a425b;
    border-radius: 12px;
}

#processInfoContainer,
#processContainer,
#previewSubContainer {
    background-color: rgba(10, 18, 27, 0.98);
    border: 1px solid #2d4662;
    border-radius: 14px;
}

#previewLabel,
#renderOutput,
#systemInfoText,
#changeLogText,
#inputVideoInfoTextEdit,
#OutputFilesListWidget,
#renderQueueListWidget {
    background-color: #101a26;
    border: 1px solid #2a3f55;
    border-radius: 10px;
}

QFrame[frameShape="4"] {
    color: #2e445c;
}
"""


def Palette():
    palette = QPalette()
    palette.setColor(QPalette.Window, QColor(15, 19, 25))
    palette.setColor(QPalette.WindowText, QColor(232, 238, 246))
    palette.setColor(QPalette.Base, QColor(17, 26, 36))
    palette.setColor(QPalette.AlternateBase, QColor(26, 38, 53))
    palette.setColor(QPalette.ToolTipBase, Qt.black)
    palette.setColor(QPalette.ToolTipText, Qt.white)
    palette.setColor(QPalette.Text, QColor(233, 241, 255))
    palette.setColor(QPalette.Button, QColor(27, 39, 53))
    palette.setColor(QPalette.ButtonText, QColor(233, 241, 255))
    palette.setColor(QPalette.Disabled, QPalette.Base, QColor(20, 29, 40))
    palette.setColor(QPalette.Disabled, QPalette.Text, QColor(112, 131, 154))
    palette.setColor(QPalette.Disabled, QPalette.Button, QColor(20, 29, 40))
    palette.setColor(QPalette.Disabled, QPalette.ButtonText, QColor(112, 131, 154))
    palette.setColor(QPalette.Disabled, QPalette.Window, QColor(15, 19, 25))
    palette.setColor(QPalette.Disabled, QPalette.WindowText, QColor(112, 131, 154))
    palette.setColor(QPalette.BrightText, Qt.red)
    palette.setColor(QPalette.Link, QColor(47, 140, 255))
    palette.setColor(QPalette.Highlight, QColor(47, 140, 255))
    palette.setColor(QPalette.HighlightedText, QColor(255, 255, 255))
    return palette


def fadeAnimation(qObject: QWidget):
    opacity_effect = QGraphicsOpacityEffect()
    qObject.setGraphicsEffect(opacity_effect)

    animation = QPropertyAnimation(opacity_effect, b"opacity")
    animation.setDuration(200)  # Duration in milliseconds
    animation.setStartValue(0.3)
    animation.setEndValue(1)
    animation.start()
