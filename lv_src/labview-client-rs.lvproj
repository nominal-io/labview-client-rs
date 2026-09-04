<?xml version='1.0' encoding='UTF-8'?>
<Project Type="Project" LVVersion="20008000">
	<Property Name="NI.LV.All.SourceOnly" Type="Bool">true</Property>
	<Property Name="NI.Project.Description" Type="Str"></Property>
	<Item Name="My Computer" Type="My Computer">
		<Property Name="NI.SortType" Type="Int">3</Property>
		<Property Name="server.app.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="server.control.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="server.tcp.enabled" Type="Bool">false</Property>
		<Property Name="server.tcp.port" Type="Int">0</Property>
		<Property Name="server.tcp.serviceName" Type="Str">My Computer/VI Server</Property>
		<Property Name="server.tcp.serviceName.default" Type="Str">My Computer/VI Server</Property>
		<Property Name="server.vi.callsEnabled" Type="Bool">true</Property>
		<Property Name="server.vi.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="specify.custom.address" Type="Bool">false</Property>
		<Item Name="dev" Type="Folder">
			<Item Name="replace subvi from template.vi" Type="VI" URL="../dev/replace subvi from template.vi"/>
			<Item Name="Replace inputs and outputs.vi" Type="VI" URL="../dev/Replace inputs and outputs.vi"/>
			<Item Name="Add controls and indicators.vi" Type="VI" URL="../dev/Add controls and indicators.vi"/>
			<Item Name="Set Display names.vi" Type="VI" URL="../dev/Set Display names.vi"/>
			<Item Name="Set descriptionvi.vi" Type="VI" URL="../dev/Set descriptionvi.vi"/>
			<Item Name="scripting.vi" Type="VI" URL="../dev/scripting.vi"/>
			<Item Name="buffer_template.vi" Type="VI" URL="../dev/buffer_template.vi"/>
			<Item Name="template.vi" Type="VI" URL="../dev/template.vi"/>
		</Item>
		<Item Name="tests" Type="Folder">
			<Item Name="01 - Offline client lifecycle.vi" Type="VI" URL="../tests/01 - Offline client lifecycle.vi"/>
			<Item Name="01 LV - Offline client lifecycle.vi" Type="VI" URL="../tests/01 LV - Offline client lifecycle.vi"/>
			<Item Name="02 - Real API list + loop.vi" Type="VI" URL="../tests/02 - Real API list + loop.vi"/>
			<Item Name="02 LV - Real API list + loop.vi" Type="VI" URL="../tests/02 LV - Real API list + loop.vi"/>
			<Item Name="03 - Staged create-verify-staged update-archive.vi" Type="VI" URL="../tests/03 - Staged create-verify-staged update-archive.vi"/>
			<Item Name="03 LV - Staged create-verify-staged update-archive.vi" Type="VI" URL="../tests/03 LV - Staged create-verify-staged update-archive.vi"/>
			<Item Name="04 - Run lifecycle.vi" Type="VI" URL="../tests/04 - Run lifecycle.vi"/>
			<Item Name="04 LV - Run lifecycle.vi" Type="VI" URL="../tests/04 LV - Run lifecycle.vi"/>
			<Item Name="05 - Dataset Lifecycle.vi" Type="VI" URL="../tests/05 - Dataset Lifecycle.vi"/>
			<Item Name="05 LV - Dataset Lifecycle.vi" Type="VI" URL="../tests/05 LV - Dataset Lifecycle.vi"/>
			<Item Name="06 - Video Lifecycle.vi" Type="VI" URL="../tests/06 - Video Lifecycle.vi"/>
			<Item Name="06 LV - Video Lifecycle.vi" Type="VI" URL="../tests/06 LV - Video Lifecycle.vi"/>
			<Item Name="07 - Channel metadata.vi" Type="VI" URL="../tests/07 - Channel metadata.vi"/>
			<Item Name="07 LV - Channel metadata.vi" Type="VI" URL="../tests/07 LV - Channel metadata.vi"/>
			<Item Name="08 - CSV Ingest.vi" Type="VI" URL="../tests/08 - CSV Ingest.vi"/>
			<Item Name="08 LV - CSV Ingest.vi" Type="VI" URL="../tests/08 LV - CSV Ingest.vi"/>
			<Item Name="09 - Workbook from template.vi" Type="VI" URL="../tests/09 - Workbook from template.vi"/>
			<Item Name="10 - Who am I.vi" Type="VI" URL="../tests/10 - Who am I.vi"/>
			<Item Name="10 LV - Who am I.vi" Type="VI" URL="../tests/10 LV - Who am I.vi"/>
			<Item Name="11 - Workspace discovery.vi" Type="VI" URL="../tests/11 - Workspace discovery.vi"/>
			<Item Name="labview-tests.md" Type="Document" URL="../tests/labview-tests.md"/>
			<Item Name="test.csv" Type="Document" URL="../tests/test.csv"/>
		</Item>
		<Item Name="client.lvlib" Type="Library" URL="../client/client.lvlib"/>
		<Item Name="AssetAttachDatasetStaging.lvclass" Type="LVClass" URL="../client/AssetAttachDatasetStaging/AssetAttachDatasetStaging.lvclass"/>
		<Item Name="AssetCreateStaging.lvclass" Type="LVClass" URL="../client/AssetCreateStaging/AssetCreateStaging.lvclass"/>
		<Item Name="AssetUpdateStaging.lvclass" Type="LVClass" URL="../client/AssetUpdateStaging/AssetUpdateStaging.lvclass"/>
		<Item Name="Channel.lvclass" Type="LVClass" URL="../client/Channel/Channel.lvclass"/>
		<Item Name="Client.lvclass" Type="LVClass" URL="../client/Client/Client.lvclass"/>
		<Item Name="DataflashIngestStaging.lvclass" Type="LVClass" URL="../client/DataflashIngestStaging/DataflashIngestStaging.lvclass"/>
		<Item Name="Dataset.lvclass" Type="LVClass" URL="../client/Dataset/Dataset.lvclass"/>
		<Item Name="DatasetCreateStaging.lvclass" Type="LVClass" URL="../client/DatasetCreateStaging/DatasetCreateStaging.lvclass"/>
		<Item Name="DatasetUpdateStaging.lvclass" Type="LVClass" URL="../client/DatasetUpdateStaging/DatasetUpdateStaging.lvclass"/>
		<Item Name="IngestJob.lvclass" Type="LVClass" URL="../client/IngestJob/IngestJob.lvclass"/>
		<Item Name="McapIngestStaging.lvclass" Type="LVClass" URL="../client/McapIngestStaging/McapIngestStaging.lvclass"/>
		<Item Name="Run.lvclass" Type="LVClass" URL="../client/Run/Run.lvclass"/>
		<Item Name="RunCreateStaging.lvclass" Type="LVClass" URL="../client/RunCreateStaging/RunCreateStaging.lvclass"/>
		<Item Name="RunUpdateStaging.lvclass" Type="LVClass" URL="../client/RunUpdateStaging/RunUpdateStaging.lvclass"/>
		<Item Name="TabularIngestStaging.lvclass" Type="LVClass" URL="../client/TabularIngestStaging/TabularIngestStaging.lvclass"/>
		<Item Name="Template.lvclass" Type="LVClass" URL="../client/Template/Template.lvclass"/>
		<Item Name="User.lvclass" Type="LVClass" URL="../client/User/User.lvclass"/>
		<Item Name="Video.lvclass" Type="LVClass" URL="../client/Video/Video.lvclass"/>
		<Item Name="VideoCreateStaging.lvclass" Type="LVClass" URL="../client/VideoCreateStaging/VideoCreateStaging.lvclass"/>
		<Item Name="VideoUpdateStaging.lvclass" Type="LVClass" URL="../client/VideoUpdateStaging/VideoUpdateStaging.lvclass"/>
		<Item Name="Workbook.lvclass" Type="LVClass" URL="../client/Workbook/Workbook.lvclass"/>
		<Item Name="WorkbookCreateStaging.lvclass" Type="LVClass" URL="../client/WorkbookCreateStaging/WorkbookCreateStaging.lvclass"/>
		<Item Name="Workspace.lvclass" Type="LVClass" URL="../client/Workspace/Workspace.lvclass"/>
		<Item Name="Asset.lvclass" Type="LVClass" URL="../client/Asset/Asset.lvclass"/>
		<Item Name="List.lvclass" Type="LVClass" URL="../client/List/List.lvclass"/>
		<Item Name="Dependencies" Type="Dependencies">
			<Item Name="vi.lib" Type="Folder">
				<Item Name="Error Cluster From Error Code.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Error Cluster From Error Code.vi"/>
				<Item Name="NI_LVConfig.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/config.llb/NI_LVConfig.lvlib"/>
				<Item Name="Trim Whitespace.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Trim Whitespace.vi"/>
				<Item Name="whitespace.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/whitespace.ctl"/>
				<Item Name="Clear Errors.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Clear Errors.vi"/>
				<Item Name="Check if File or Folder Exists.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/libraryn.llb/Check if File or Folder Exists.vi"/>
				<Item Name="NI_FileType.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/lvfile.llb/NI_FileType.lvlib"/>
				<Item Name="NI_PackedLibraryUtility.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/LVLibp/NI_PackedLibraryUtility.lvlib"/>
				<Item Name="8.6CompatibleGlobalVar.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/config.llb/8.6CompatibleGlobalVar.vi"/>
				<Item Name="Simple Error Handler.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Simple Error Handler.vi"/>
				<Item Name="DialogType.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/DialogType.ctl"/>
				<Item Name="General Error Handler.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/General Error Handler.vi"/>
				<Item Name="DialogTypeEnum.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/DialogTypeEnum.ctl"/>
				<Item Name="General Error Handler Core CORE.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/General Error Handler Core CORE.vi"/>
				<Item Name="Check Special Tags.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Check Special Tags.vi"/>
				<Item Name="TagReturnType.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/TagReturnType.ctl"/>
				<Item Name="Set String Value.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Set String Value.vi"/>
				<Item Name="GetRTHostConnectedProp.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/GetRTHostConnectedProp.vi"/>
				<Item Name="Error Code Database.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Error Code Database.vi"/>
				<Item Name="Format Message String.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Format Message String.vi"/>
				<Item Name="Find Tag.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Find Tag.vi"/>
				<Item Name="Search and Replace Pattern.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Search and Replace Pattern.vi"/>
				<Item Name="Set Bold Text.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Set Bold Text.vi"/>
				<Item Name="Details Display Dialog.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Details Display Dialog.vi"/>
				<Item Name="ErrWarn.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/ErrWarn.ctl"/>
				<Item Name="eventvkey.ctl" Type="VI" URL="/&lt;vilib&gt;/event_ctls.llb/eventvkey.ctl"/>
				<Item Name="Not Found Dialog.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Not Found Dialog.vi"/>
				<Item Name="Three Button Dialog.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Three Button Dialog.vi"/>
				<Item Name="Three Button Dialog CORE.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Three Button Dialog CORE.vi"/>
				<Item Name="LVRectTypeDef.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/miscctls.llb/LVRectTypeDef.ctl"/>
				<Item Name="Longest Line Length in Pixels.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Longest Line Length in Pixels.vi"/>
				<Item Name="Convert property node font to graphics font.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Convert property node font to graphics font.vi"/>
				<Item Name="Get Text Rect.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Get Text Rect.vi"/>
				<Item Name="Get String Text Bounds.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Get String Text Bounds.vi"/>
				<Item Name="LVBoundsTypeDef.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/miscctls.llb/LVBoundsTypeDef.ctl"/>
				<Item Name="BuildHelpPath.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/BuildHelpPath.vi"/>
				<Item Name="GetHelpDir.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/GetHelpDir.vi"/>
				<Item Name="TRef TravTarget.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/traverseref.llb/TRef TravTarget.ctl"/>
				<Item Name="Application Directory.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Application Directory.vi"/>
				<Item Name="VI Scripting - Traverse.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/traverseref.llb/VI Scripting - Traverse.lvlib"/>
				<Item Name="TRef Traverse.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/traverseref.llb/TRef Traverse.vi"/>
				<Item Name="Write Delimited Spreadsheet.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet.vi"/>
				<Item Name="Write Delimited Spreadsheet (DBL).vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet (DBL).vi"/>
				<Item Name="Write Spreadsheet String.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Spreadsheet String.vi"/>
				<Item Name="Write Delimited Spreadsheet (I64).vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet (I64).vi"/>
				<Item Name="Write Delimited Spreadsheet (string).vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet (string).vi"/>
				<Item Name="Get File Extension.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/libraryn.llb/Get File Extension.vi"/>
				<Item Name="Get GObject Label.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/traverseref.llb/Get GObject Label.vi"/>
				<Item Name="TRef Find Object By Label.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/traverseref.llb/TRef Find Object By Label.vi"/>
				<Item Name="Get Owning Structure of Terminal.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/QuickDropSupport/Get Owning Structure of Terminal.vi"/>
				<Item Name="LVPositionTypeDef.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/miscctls.llb/LVPositionTypeDef.ctl"/>
				<Item Name="NI_Data Type.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/Data Type/NI_Data Type.lvlib"/>
				<Item Name="VIAnUtil Check Type If ErrClust.vi" Type="VI" URL="/&lt;vilib&gt;/addons/analyzer/_analyzerutils.llb/VIAnUtil Check Type If ErrClust.vi"/>
				<Item Name="VIAnUtil Get Terminal Data Type.vi" Type="VI" URL="/&lt;vilib&gt;/addons/analyzer/_analyzerutils.llb/VIAnUtil Get Terminal Data Type.vi"/>
				<Item Name="Wire All Terminals_core.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/QuickDropSupport/Wire All Terminals_core.vi"/>
				<Item Name="LayerType.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/LayerType.ctl"/>
				<Item Name="Layer.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/Layer.ctl"/>
				<Item Name="LVPoint32TypeDef.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/miscctls.llb/LVPoint32TypeDef.ctl"/>
				<Item Name="Layer.lvclass" Type="LVClass" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Classes/Layer/Layer.lvclass"/>
				<Item Name="Alignment.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/Alignment.ctl"/>
				<Item Name="Font.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/Font.ctl"/>
				<Item Name="BodyText.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/BodyText.ctl"/>
				<Item Name="Icon Framework.lvclass" Type="LVClass" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Classes/Icon Framework/Icon Framework.lvclass"/>
				<Item Name="Icon.lvclass" Type="LVClass" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Classes/Icon/Icon.lvclass"/>
				<Item Name="FixBadRect.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pictutil.llb/FixBadRect.vi"/>
				<Item Name="imagedata.ctl" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/imagedata.ctl"/>
				<Item Name="Draw Flattened Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw Flattened Pixmap.vi"/>
				<Item Name="Bit-array To Byte-array.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pictutil.llb/Bit-array To Byte-array.vi"/>
				<Item Name="Unflatten Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pixmap.llb/Unflatten Pixmap.vi"/>
				<Item Name="Create Mask.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pictutil.llb/Create Mask.vi"/>
				<Item Name="Picture to Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pictutil.llb/Picture to Pixmap.vi"/>
				<Item Name="lv_icon.lvlib" Type="Library" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/lv_icon.lvlib"/>
				<Item Name="PCT Pad String.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/PCT Pad String.vi"/>
				<Item Name="Draw Text in Rect.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw Text in Rect.vi"/>
				<Item Name="Draw Text at Point.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw Text at Point.vi"/>
				<Item Name="Empty Picture" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Empty Picture"/>
				<Item Name="Color to RGB.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/colorconv.llb/Color to RGB.vi"/>
				<Item Name="Flatten Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pixmap.llb/Flatten Pixmap.vi"/>
				<Item Name="Draw True-Color Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw True-Color Pixmap.vi"/>
				<Item Name="Draw 1-Bit Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw 1-Bit Pixmap.vi"/>
				<Item Name="Draw 8-Bit Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw 8-Bit Pixmap.vi"/>
				<Item Name="Draw 4-Bit Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw 4-Bit Pixmap.vi"/>
				<Item Name="Draw Unflattened Pixmap.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw Unflattened Pixmap.vi"/>
				<Item Name="RGB to Color.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/colorconv.llb/RGB to Color.vi"/>
				<Item Name="Graphic.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/Graphic.ctl"/>
				<Item Name="IEColor.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/IEColor.ctl"/>
				<Item Name="BodyTextPosition.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/BodyTextPosition.ctl"/>
				<Item Name="Set Pen State.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Set Pen State.vi"/>
				<Item Name="Draw Rect.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw Rect.vi"/>
				<Item Name="LabVIEW Icon Stored Information.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/LabVIEW Icon Stored Information.ctl"/>
				<Item Name="Load &amp; Unload.lvclass" Type="LVClass" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Classes/Load_Unload/Load &amp; Unload.lvclass"/>
				<Item Name="Coerce Bad Rect.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pictutil.llb/Coerce Bad Rect.vi"/>
				<Item Name="Get Image Subset.vi" Type="VI" URL="/&lt;vilib&gt;/picture/pictutil.llb/Get Image Subset.vi"/>
				<Item Name="LabVIEW Icon API.lvlib" Type="Library" URL="/&lt;vilib&gt;/LabVIEW Icon API/LabVIEW Icon API.lvlib"/>
				<Item Name="Compare Two Paths.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/libraryn.llb/Compare Two Paths.vi"/>
				<Item Name="Dflt Data Dir.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Dflt Data Dir.vi"/>
				<Item Name="Pathes.ctl" Type="VI" URL="/&lt;vilib&gt;/LabVIEW Icon API/lv_icon/Controls/Pathes.ctl"/>
				<Item Name="Text-Based VI Icon.lvlib" Type="Library" URL="/&lt;vilib&gt;/LabVIEW Icon API/Set Text Icon/Text-Based VI Icon.lvlib"/>
				<Item Name="Space Constant.vi" Type="VI" URL="/&lt;vilib&gt;/dlg_ctls.llb/Space Constant.vi"/>
				<Item Name="Draw Rectangle.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Draw Rectangle.vi"/>
				<Item Name="Stall Data Flow.vim" Type="VI" URL="/&lt;vilib&gt;/Utility/Stall Data Flow.vim"/>
			</Item>
			<Item Name="nominalClient_64.dll" Type="Document" URL="../bin/nominalClient_64.dll"/>
			<Item Name="zzRemove (no error).ctl" Type="VI" URL="/&lt;resource&gt;/plugins/PopupMenus/edit time panel and diagram/zzRemove (no error).llb/zzRemove (no error).ctl"/>
			<Item Name="Execute zzRemove (no error).vi" Type="VI" URL="/&lt;resource&gt;/plugins/PopupMenus/edit time panel and diagram/zzRemove (no error).llb/Execute zzRemove (no error).vi"/>
			<Item Name="LV Config Read Boolean.vi" Type="VI" URL="/&lt;resource&gt;/dialog/lvconfig.llb/LV Config Read Boolean.vi"/>
		</Item>
		<Item Name="Build Specifications" Type="Build"/>
	</Item>
</Project>
